use std::net::SocketAddr;
use std::path::PathBuf;

use ti4_advisor::Advisor;

const DEFAULT_CHECKPOINT: &str = "examples/reviewer/checkpoint-473312";
const DEFAULT_PORT: u16 = 8081;
const TENSOR_SEED: i64 = 20_260_821;

#[tokio::main]
async fn main() {
    let mut checkpoint = PathBuf::from(DEFAULT_CHECKPOINT);
    let mut port = DEFAULT_PORT;
    let mut arguments = std::env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--checkpoint" => match arguments.next() {
                Some(value) => checkpoint = PathBuf::from(value),
                None => usage("--checkpoint requires a directory"),
            },
            "--port" => match arguments.next().and_then(|value| value.parse().ok()) {
                Some(value) => port = value,
                None => usage("--port requires a valid u16 port"),
            },
            "--help" | "-h" => usage(""),
            _ => usage("unknown argument"),
        }
    }

    if let Err(error) = ti4_tensor::configure_deterministic(TENSOR_SEED) {
        eprintln!("cannot configure deterministic tensor backend: {error}");
        std::process::exit(2);
    }

    let advisor = match Advisor::load(&checkpoint) {
        Ok(advisor) => advisor,
        Err(error) => {
            eprintln!("cannot load checkpoint {}: {error}", checkpoint.display());
            std::process::exit(2);
        }
    };
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot bind advisor to {address}: {error}");
            std::process::exit(2);
        }
    };

    if let Err(error) = axum::serve(listener, advisor.router()).await {
        eprintln!("advisor server stopped: {error}");
        std::process::exit(1);
    }
}

fn usage(reason: &str) -> ! {
    if !reason.is_empty() {
        eprintln!("{reason}");
    }
    eprintln!("usage: ti4-advisor [--checkpoint DIR] [--port PORT]");
    std::process::exit(if reason.is_empty() { 0 } else { 2 });
}
