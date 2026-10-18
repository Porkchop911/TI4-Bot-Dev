//! `selfplay`: games with the rule bot on every seat, no server, no browser.
//! Run `selfplay help`, or read `docs/RULE_BOT_OPTIMISATION.md`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use ti4_selfplay::driver::{expand, run_matrix, trace_text, tuning_from};
use ti4_selfplay::play::{GameResult, PlayOptions, TraceLine, play_guarded};
use ti4_selfplay::run::{compare, render_comparison};
use ti4_selfplay::spec::{
    GameSpec, Limits, MapPolicy, Matrix, Rotate, Seeds, SetupSpec, parse_seeds,
};
use ti4_selfplay::stats::{Summary, render};

const HELP: &str = "\
selfplay: rule-bot-only games (see docs/RULE_BOT_OPTIMISATION.md)

  selfplay run      [setup flags] [run flags]    play games, print and write a summary
  selfplay compare  [setup flags] --candidate k=v,k=v [--baseline k=v] [--mode mixed|separate]
  selfplay replay   one game by seed/spec/summary; --trace FILE, --text, --expect-hash H
  selfplay knobs                                  the bot's tunable numbers and their defaults
  selfplay maps     [--players N]                 map templates that build for a table size

setup flags (one setup per --players value; or --config FILE for a matrix)
  --players 4,6          table sizes, 3..8 (default 4,6)
  --games N              seeds 0..N per setup (default 10)       --seed-start S
  --seeds 1..50|3,5,9    explicit seeds (overrides --games)
  --map a,b,random       template aliases and/or random (default: recommended for the size)
  --map-policy cycle|random|cross   how maps share the seeds (default cycle)
  --factions sol,_,hacan fix seats in seat order; _ = free (rest random, no duplicates)
  --faction-pool a,b,c   draw the free seats from this pool
  --rotate none|cyclic|all  play each table with the factions rotated over the seats
  --speaker seat|random|N   (default random)        --card-set pok|te|...
  --starting-tech        the seats that choose a starting technology are asked (bot version 3 chooses by plan)
limits
  --max-rounds 50  --max-steps 400000  --max-seconds 300  --loop-steps 5000
run flags
  --workers N (default cores-1)   --bot key=val[,key=val]   --bot-config FILE.json
  --out FILE.json   --games-out FILE.jsonl   --trace-dir DIR [--trace-which bad|all]
  --verify BASELINE.json   --top N   --quiet   --no-table
";

struct Args {
    values: BTreeMap<String, Vec<String>>,
    positional: Vec<String>,
}

impl Args {
    fn parse(raw: &[String]) -> Result<Self, String> {
        const SWITCHES: &[&str] = &[
            "quiet",
            "no-table",
            "text",
            "help",
            "starting-tech",
            "no-cache",
        ];
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut positional = Vec::new();
        let mut it = raw.iter();
        while let Some(a) = it.next() {
            if let Some(name) = a.strip_prefix("--") {
                if let Some((k, v)) = name.split_once('=') {
                    values.entry(k.to_owned()).or_default().push(v.to_owned());
                } else if SWITCHES.contains(&name) {
                    values
                        .entry(name.to_owned())
                        .or_default()
                        .push(String::new());
                } else {
                    let v = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
                    values.entry(name.to_owned()).or_default().push(v.clone());
                }
            } else {
                positional.push(a.clone());
            }
        }
        Ok(Self { values, positional })
    }

    fn get(&self, k: &str) -> Option<&str> {
        self.values
            .get(k)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    fn has(&self, k: &str) -> bool {
        self.values.contains_key(k)
    }

    fn list(&self, k: &str) -> Vec<String> {
        self.values
            .get(k)
            .into_iter()
            .flatten()
            .flat_map(|v| v.split(','))
            .map(|s| s.trim().to_owned())
            .collect()
    }

    fn num<T: std::str::FromStr>(&self, k: &str) -> Result<Option<T>, String>
    where
        T::Err: std::fmt::Display,
    {
        self.get(k)
            .map(|v| v.parse::<T>().map_err(|e| format!("--{k} {v}: {e}")))
            .transpose()
    }
}

fn limits_of(args: &Args, base: Limits) -> Result<Limits, String> {
    let mut l = base;
    if let Some(v) = args.num("max-rounds")? {
        l.max_rounds = v;
    }
    if let Some(v) = args.num("max-steps")? {
        l.max_steps = v;
    }
    if let Some(v) = args.num("max-seconds")? {
        l.max_seconds = v;
    }
    if let Some(v) = args.num("loop-steps")? {
        l.loop_steps = v;
    }
    Ok(l)
}

/// `--rules-version N`: pins the rules version of the games ([`GameSpec::rules_version`]).
fn rules_version_of(args: &Args) -> Result<Option<u32>, String> {
    args.get("rules-version")
        .map(|v| v.parse().map_err(|_| format!("--rules-version {v}: not a number")))
        .transpose()
}

fn bot_of(
    args: &Args,
    mut base: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    if let Some(path) = args.get("bot-config") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let v: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        for (k, v) in v {
            base.insert(k, v.as_str().map_or_else(|| v.to_string(), str::to_owned));
        }
    }
    base.extend(kv_list(&args.list("bot"))?);
    Ok(base)
}

fn kv_list(items: &[String]) -> Result<BTreeMap<String, String>, String> {
    items
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
                .ok_or_else(|| format!("'{s}': use key=value"))
        })
        .collect()
}

/// The setups the flags describe.
fn setups_of(args: &Args) -> Result<Vec<SetupSpec>, String> {
    let players: Vec<usize> = if args.has("players") {
        args.list("players")
            .iter()
            .map(|p| p.parse().map_err(|e| format!("--players {p}: {e}")))
            .collect::<Result<_, _>>()?
    } else {
        vec![4, 6]
    };
    let seeds = if let Some(s) = args.get("seeds") {
        Seeds::List(parse_seeds(s)?)
    } else {
        let start: u64 = args.num("seed-start")?.unwrap_or(0);
        let count: u64 = args.num("games")?.unwrap_or(10);
        Seeds::List((start..start + count).collect())
    };
    let named = players.len() > 1 || args.has("name");
    players
        .iter()
        .map(|&p| {
            Ok(SetupSpec {
                name: if named {
                    Some(args.get("name").map_or_else(
                        || format!("{p}p"),
                        |n| {
                            if players.len() > 1 {
                                format!("{n}-{p}p")
                            } else {
                                n.to_owned()
                            }
                        },
                    ))
                } else {
                    None
                },
                players: p,
                maps: args
                    .list("map")
                    .into_iter()
                    .filter(|m| !m.is_empty())
                    .collect(),
                map_policy: args
                    .get("map-policy")
                    .map(MapPolicy::parse)
                    .transpose()?
                    .unwrap_or_default(),
                factions: args
                    .list("factions")
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .filter(|_| args.has("factions"))
                    .collect(),
                faction_pool: args
                    .list("faction-pool")
                    .into_iter()
                    .filter(|m| !m.is_empty())
                    .collect(),
                rotate: args
                    .get("rotate")
                    .map(Rotate::parse)
                    .transpose()?
                    .unwrap_or_default(),
                speaker: args.get("speaker").unwrap_or("random").to_owned(),
                card_set: args.get("card-set").map(str::to_owned),
                starting_tech: args.has("starting-tech"),
                rules_version: rules_version_of(args)?,
                seeds: seeds.clone(),
            })
        })
        .collect()
}

fn matrix_of(args: &Args) -> Result<Matrix, String> {
    let mut matrix = if let Some(path) = args.get("config") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        serde_json::from_str::<Matrix>(&text).map_err(|e| format!("{path}: {e}"))?
    } else {
        Matrix {
            setups: setups_of(args)?,
            ..Matrix::default()
        }
    };
    matrix.limits = limits_of(args, matrix.limits.clone())?;
    matrix.bot = bot_of(args, matrix.bot.clone())?;
    Ok(matrix)
}

fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1))
}

fn write_json<T: serde::Serialize>(path: &str, value: &T) -> Result<(), String> {
    if let Some(dir) = std::path::Path::new(path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{path}: {e}"))
}

fn trace_path(dir: &str, r: &GameResult) -> PathBuf {
    let s = &r.spec;
    PathBuf::from(dir).join(format!(
        "{}_{}p_{}_seed{}{}.jsonl",
        s.setup,
        s.players,
        s.map,
        s.seed,
        s.rotation.map_or(String::new(), |r| format!("_rot{r}"))
    ))
}

fn write_trace(path: &std::path::Path, r: &GameResult, trace: &[TraceLine]) {
    let mut out = String::new();
    out.push_str(&serde_json::json!({"spec": r.spec, "termination": r.termination, "detail": r.detail, "hash": r.hash, "vp": r.vp}).to_string());
    out.push('\n');
    for t in trace {
        out.push_str(&serde_json::to_string(t).expect("a trace line serialises"));
        out.push('\n');
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(path, out);
}

fn cmd_run(args: &Args) -> Result<ExitCode, String> {
    let matrix = matrix_of(args)?;
    let workers = args.num("workers")?.unwrap_or_else(default_workers);
    let quiet = args.has("quiet");
    let trace_dir = args.get("trace-dir").map(str::to_owned);
    let which_all = args.get("trace-which") == Some("all");
    let hook = |r: &GameResult, trace: Option<&[TraceLine]>| {
        if let (Some(dir), Some(trace)) = (&trace_dir, trace) {
            if which_all || !matches!(r.termination.as_str(), "victory") {
                write_trace(&trace_path(dir, r), r, trace);
            }
        }
    };
    let (summary, results) =
        run_matrix(&matrix, workers, !quiet, trace_dir.is_some(), Some(&hook))?;
    if let Some(path) = args.get("out") {
        write_json(path, &summary)?;
    }
    if let Some(path) = args.get("games-out") {
        let lines: Vec<String> = results
            .iter()
            .map(|g| serde_json::to_string(g).expect("serialises"))
            .collect();
        std::fs::write(path, lines.join("\n") + "\n").map_err(|e| format!("{path}: {e}"))?;
    }
    if !args.has("no-table") {
        print!("{}", render(&summary, args.num("top")?.unwrap_or(14)));
    }
    if let Some(path) = args.get("verify") {
        return verify(&summary, path);
    }
    Ok(ExitCode::SUCCESS)
}

/// Compares the game hashes of this run with a summary written earlier.
fn verify(summary: &Summary, path: &str) -> Result<ExitCode, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let base: Summary = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let (mut same, mut different, mut missing) = (0, Vec::new(), 0);
    for g in &summary.games {
        // A baseline recorded before rules versions could be pinned carries no `rules_version`:
        // a pinned game is compared with it by the rest of its spec.
        let unpinned = GameSpec {
            rules_version: None,
            ..g.spec.clone()
        };
        match base
            .games
            .iter()
            .find(|b| b.spec == g.spec || (g.spec.rules_version.is_some() && b.spec == unpinned))
        {
            None => missing += 1,
            Some(b) if b.hash == g.hash && b.termination == g.termination && b.vp == g.vp => {
                same += 1
            }
            Some(_) => different.push(g.spec.clone()),
        }
    }
    println!(
        "verify against {path}: {same} identical, {} different, {missing} not in the baseline",
        different.len()
    );
    for spec in different.iter().take(20) {
        println!("  DIFFERENT: selfplay replay {}", spec.command_line());
    }
    Ok(if different.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn cmd_compare(args: &Args) -> Result<ExitCode, String> {
    let matrix = matrix_of(args)?;
    let (specs, skipped) = expand(&matrix)?;
    for s in &skipped {
        eprintln!("skipped: {s}");
    }
    let mut cand = matrix.bot.clone();
    cand.extend(kv_list(&args.list("candidate"))?);
    if let Some(path) = args.get("candidate-config") {
        let v: BTreeMap<String, serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
        )
        .map_err(|e| format!("{path}: {e}"))?;
        cand.extend(
            v.into_iter()
                .map(|(k, v)| (k, v.as_str().map_or_else(|| v.to_string(), str::to_owned))),
        );
    }
    let base = kv_list(&args.list("baseline"))?;
    let mixed = match args.get("mode").unwrap_or("mixed") {
        "mixed" => true,
        "separate" => false,
        other => return Err(format!("--mode {other}: use mixed or separate")),
    };
    let (c, _) = compare(
        &specs,
        &matrix.limits,
        tuning_from(&base)?,
        tuning_from(&cand)?,
        mixed,
        args.num("workers")?.unwrap_or_else(default_workers),
        !args.has("quiet"),
    );
    if let Some(path) = args.get("out") {
        write_json(path, &c)?;
    }
    print!("{}", render_comparison(&c));
    Ok(ExitCode::SUCCESS)
}

/// `selfplay metrics FILE`: the A/B metrics of a stored `compare` run, without playing.
fn cmd_metrics(args: &Args) -> Result<ExitCode, String> {
    let path = args
        .positional
        .first()
        .ok_or("usage: selfplay metrics FILE.jsonl (a stored run, see docs)")?;
    let results = ti4_selfplay::store::read_file(std::path::Path::new(path))?;
    let mixed = results.iter().any(|g| {
        g.candidate_seats
            .as_ref()
            .is_some_and(|m| m.iter().any(|x| *x) && m.iter().any(|x| !*x))
    });
    let (c, _) = ti4_selfplay::run::comparison_from(
        results,
        ti4_rule_bot::tuning::current(),
        ti4_rule_bot::tuning::current(),
        mixed,
        std::time::Instant::now(),
    );
    print!("{}", render_comparison(&c));
    if let Some(out) = args.get("out") {
        write_json(out, &c)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_replay(args: &Args) -> Result<ExitCode, String> {
    let spec: GameSpec = if let Some(text) = args.get("spec") {
        serde_json::from_str(text).map_err(|e| format!("--spec: {e}"))?
    } else if let Some(path) = args.get("summary") {
        let base: Summary = serde_json::from_str(
            &std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
        )
        .map_err(|e| format!("{path}: {e}"))?;
        let seed: u64 = args.num("seed")?.ok_or("--summary needs --seed")?;
        let nth: usize = args.num("nth")?.unwrap_or(0);
        base.games
            .iter()
            .filter(|g| g.spec.seed == seed && args.get("setup").is_none_or(|s| g.spec.setup == s))
            .nth(nth)
            .map(|g| g.spec.clone())
            .ok_or("no such game in the summary")?
    } else {
        let seed: u64 = args
            .num("seed")?
            .ok_or("replay needs --seed (or --spec, or --summary and --seed)")?;
        let players: usize = args.num("players")?.unwrap_or(4);
        let setup = SetupSpec {
            name: Some("replay".to_owned()),
            players,
            maps: args
                .list("map")
                .into_iter()
                .filter(|m| !m.is_empty())
                .collect(),
            map_policy: MapPolicy::Cycle,
            factions: if args.has("factions") {
                args.list("factions")
            } else {
                vec![]
            },
            faction_pool: args
                .list("faction-pool")
                .into_iter()
                .filter(|m| !m.is_empty())
                .collect(),
            rotate: Rotate::None,
            speaker: args.get("speaker").unwrap_or("random").to_owned(),
            card_set: args.get("card-set").map(str::to_owned),
            starting_tech: args.has("starting-tech"),
            rules_version: rules_version_of(args)?,
            seeds: Seeds::List(vec![seed]),
        };
        setup.expand()?.0.into_iter().next().ok_or("no game")?
    };
    let bot = bot_of(args, BTreeMap::new())?;
    let options = PlayOptions {
        limits: limits_of(args, Limits::default())?,
        tunings: vec![tuning_from(&bot)?],
        candidate_seats: None,
        trace: true,
        timeline: false,
    };
    ti4_selfplay::play::install_panic_capture();
    println!("game: selfplay replay {}", spec.command_line());
    let (r, trace) = play_guarded(&spec, &options);
    let trace = trace.unwrap_or_default();
    if let Some(path) = args.get("trace") {
        write_trace(std::path::Path::new(path), &r, &trace);
        println!("trace written to {path}");
    }
    if args.has("text") {
        let from: usize = args.num("from")?.unwrap_or(0);
        let to: usize = args.num("to")?.unwrap_or(usize::MAX);
        let seat = args.get("seat");
        let filtered: Vec<TraceLine> = trace
            .iter()
            .filter(|t| t.i >= from && t.i < to && seat.is_none_or(|s| t.seat == s))
            .cloned()
            .collect();
        print!("{}", trace_text(&filtered));
    }
    println!(
        "{}: winner {:?} ({}) vp {:?} rounds {} decisions {} steps {} wall {:.0} ms\nhash {}",
        r.termination,
        r.winner
            .map(|w| ["a", "b", "c", "d", "e", "f", "g", "h"][w]),
        r.winner_faction.as_deref().unwrap_or("-"),
        r.vp,
        r.rounds,
        r.decisions,
        r.steps,
        r.wall_ms,
        r.hash
    );
    if let Some(d) = &r.detail {
        println!("detail: {d}");
    }
    if let Some(want) = args.get("expect-hash") {
        if want == r.hash {
            println!("OK: the hash is the expected one");
        } else {
            println!("DIFFERENT: expected {want}");
            return Ok(ExitCode::from(2));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_knobs() {
    let t = ti4_rule_bot::tuning::current().entries();
    for ((k, v), (_, doc)) in t.iter().zip(ti4_rule_bot::tuning::KEYS) {
        println!("{k:<22} {v:<8} {doc}");
    }
}

fn cmd_maps(args: &Args) -> Result<(), String> {
    let players: Option<usize> = args.num("players")?;
    for n in players.map_or_else(|| (3..=8).collect(), |p| vec![p]) {
        let list = ti4_host::maps::catalog::catalog(Some(n))?;
        let rec = ti4_host::maps::catalog::recommended_for(n);
        println!(
            "{n} players ({} templates; recommended {}):",
            list.len(),
            rec.as_deref().unwrap_or("-")
        );
        println!(
            "  {}",
            list.iter()
                .map(|m| m.alias.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match raw.split_first() {
        Some((c, r)) if !c.starts_with("--") => (c.as_str(), r.to_vec()),
        _ => ("run", raw.clone()),
    };
    let result = Args::parse(&rest).and_then(|args| {
        if let Some(list) = args.get("exclude-sources") {
            ti4_selfplay::controllable::configure(Some(
                list.split(',').map(|s| s.trim().to_owned()).collect(),
            ));
        }
        ti4_selfplay::store::configure(if args.has("no-cache") {
            None
        } else {
            args.get("results")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var("SELFPLAY_RESULTS").map_or_else(
                        |_| Some(std::path::PathBuf::from("selfplay-results")),
                        |v| (!v.is_empty() && v != "off").then(|| std::path::PathBuf::from(v)),
                    )
                })
        });
        if args.has("help") || cmd == "help" {
            print!("{HELP}");
            return Ok(ExitCode::SUCCESS);
        }
        let _ = &args.positional;
        match cmd {
            "run" => cmd_run(&args),
            "compare" => cmd_compare(&args),
            "replay" => cmd_replay(&args),
            "metrics" => cmd_metrics(&args),
            "knobs" => {
                cmd_knobs();
                Ok(ExitCode::SUCCESS)
            }
            "maps" => cmd_maps(&args).map(|()| ExitCode::SUCCESS),
            other => Err(format!("unknown command '{other}'; try `selfplay help`")),
        }
    });
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("selfplay: {e}");
            ExitCode::from(1)
        }
    }
}
