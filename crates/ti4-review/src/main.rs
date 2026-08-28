use std::env;
use std::path::{Path, PathBuf};

use ti4_review::{
    AdvanceUnit, LiveReview, ProfileTable, SimulationConfig, export_html, gui, load_session,
    save_session,
};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        if let Err(error) = gui::run() {
            eprintln!("ti4-review: GUI failed: {error}");
            std::process::exit(2);
        }
        return;
    }
    if let Err(error) = command(&args) {
        eprintln!("ti4-review: {error}");
        std::process::exit(2);
    }
}

fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("validate") => {
            let path = required_positional(args, 1, "review path")?;
            let session = load_session(Path::new(path)).map_err(|error| error.to_string())?;
            println!(
                "valid: {} frames, {:?}",
                session.frames.len(),
                session.outcome
            );
            Ok(())
        }
        Some("render") => {
            let input = required_positional(args, 1, "review path")?;
            let output = required_positional(args, 2, "HTML path")?;
            let session = load_session(Path::new(input)).map_err(|error| error.to_string())?;
            export_html(Path::new(output), &session).map_err(|error| error.to_string())?;
            println!("rendered: {output}");
            Ok(())
        }
        Some("simulate") => simulate(args),
        _ => Err(usage()),
    }
}

fn simulate(args: &[String]) -> Result<(), String> {
    let checkpoint = flag(args, "--checkpoint").ok_or_else(usage)?;
    let map_pool = flag(args, "--map-pool").ok_or_else(usage)?;
    let out = flag(args, "--out").ok_or_else(usage)?;
    let seed = parse_flag::<u64>(args, "--seed")?.unwrap_or(42);
    let rotation = parse_flag::<usize>(args, "--rotation")?.unwrap_or(0);
    let table = match flag(args, "--table").as_deref().unwrap_or("learner") {
        "learner" => ProfileTable::Learner,
        "accepted" => ProfileTable::Accepted,
        value => {
            return Err(format!(
                "--table must be learner or accepted, got {value:?}"
            ));
        }
    };
    let unit = match flag(args, "--unit").as_deref().unwrap_or("step") {
        "step" | "steps" => AdvanceUnit::Step,
        "decision" | "decisions" => AdvanceUnit::Decision,
        "action" | "actions" => AdvanceUnit::Action,
        value => {
            return Err(format!(
                "--unit must be step, decision, or action, got {value:?}"
            ));
        }
    };
    let count = parse_flag::<usize>(args, "--count")?.unwrap_or(1);
    let config = SimulationConfig {
        checkpoint: PathBuf::from(checkpoint),
        map_pool: PathBuf::from(map_pool),
        seed,
        rotation,
        table,
    };
    let mut review = LiveReview::start(&config).map_err(|error| error.to_string())?;
    let report = match flag(args, "--until").as_deref() {
        None => review.advance(unit, count),
        Some("round") => review.advance_to_next_round(),
        Some("end") => review.advance_to_end(),
        Some(value) => return Err(format!("--until must be round or end, got {value:?}")),
    };
    save_session(Path::new(&out), &review.session).map_err(|error| error.to_string())?;
    println!(
        "saved {} frames to {out}; steps={} decisions={} actions={} target={} outcome={:?}",
        review.session.frames.len(),
        report.steps,
        report.decisions,
        report.actions,
        report.reached_target,
        review.session.outcome
    );
    Ok(())
}

fn required_positional<'a>(
    args: &'a [String],
    index: usize,
    label: &str,
) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {label}\n{}", usage()))
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn parse_flag<T>(args: &[String], name: &str) -> Result<Option<T>, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    flag(args, name)
        .map(|value| {
            value
                .parse()
                .map_err(|error| format!("invalid {name} {value:?}: {error}"))
        })
        .transpose()
}

fn usage() -> String {
    "usage:\n  ti4-review\n  ti4-review validate <game.ti4review.json>\n  ti4-review render <game.ti4review.json> <game.html>\n  ti4-review simulate --checkpoint <checkpoint.json> --map-pool <pool.json.gz> --out <game.ti4review.json> [--seed 42] [--rotation 0] [--table learner|accepted] [--unit step|decision|action] [--count N | --until round|end]".to_owned()
}
