use std::time::Duration;

use ti4_bot_agent::{BotConfig, run};

#[tokio::main]
async fn main() {
    let config = match parse_arguments() {
        Ok(config) => config,
        Err(message) => usage(&message),
    };
    if let Err(error) = run(config).await {
        eprintln!("bot agent failed: {error}");
        std::process::exit(1);
    }
}

fn parse_arguments() -> Result<BotConfig, String> {
    let mut server = None;
    let mut game_id = None;
    let mut advisor = None;
    let mut nickname = None;
    let mut temperature = None;
    let mut sample_seed = None;
    let mut timeout_seconds = None;
    let mut max_reconnects = None;
    let mut arguments = std::env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--server" => server = Some(next_value("--server", &mut arguments)?),
            "--game" => game_id = Some(next_value("--game", &mut arguments)?),
            "--advisor" => advisor = Some(next_value("--advisor", &mut arguments)?),
            "--nickname" => nickname = Some(next_value("--nickname", &mut arguments)?),
            "--temperature" => {
                temperature = Some(
                    next_value("--temperature", &mut arguments)?
                        .parse::<f64>()
                        .map_err(|_| "--temperature must be a number".to_owned())?,
                );
            }
            "--sample-seed" => {
                sample_seed = Some(
                    next_value("--sample-seed", &mut arguments)?
                        .parse::<u64>()
                        .map_err(|_| "--sample-seed must be a u64".to_owned())?,
                );
            }
            "--timeout-seconds" => {
                timeout_seconds = Some(
                    next_value("--timeout-seconds", &mut arguments)?
                        .parse::<u64>()
                        .map_err(|_| "--timeout-seconds must be a u64".to_owned())?,
                );
            }
            "--max-reconnects" => {
                max_reconnects = Some(
                    next_value("--max-reconnects", &mut arguments)?
                        .parse::<u32>()
                        .map_err(|_| "--max-reconnects must be a u32".to_owned())?,
                );
            }
            "--help" | "-h" => usage(""),
            _ => return Err(format!("unknown argument {argument}")),
        }
    }

    let mut config = BotConfig::new(
        server.ok_or_else(|| "--server is required".to_owned())?,
        game_id.ok_or_else(|| "--game is required".to_owned())?,
        advisor.ok_or_else(|| "--advisor is required".to_owned())?,
        nickname.ok_or_else(|| "--nickname is required".to_owned())?,
    );
    if let Some(value) = temperature {
        config.temperature = value;
    }
    config.sample_seed = sample_seed;
    if let Some(value) = timeout_seconds {
        config.timeout = Duration::from_secs(value);
    }
    if let Some(value) = max_reconnects {
        config.max_reconnects = value;
    }
    Ok(config)
}

fn next_value<I>(name: &str, arguments: &mut I) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    arguments
        .next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn usage(reason: &str) -> ! {
    if !reason.is_empty() {
        eprintln!("{reason}");
    }
    eprintln!(
        "usage: ti4-bot-agent --server ws://HOST:PORT --game ID --advisor http://HOST:PORT --nickname NAME [--temperature F64] [--sample-seed U64] [--timeout-seconds U64] [--max-reconnects U32]"
    );
    std::process::exit(if reason.is_empty() { 0 } else { 2 });
}
