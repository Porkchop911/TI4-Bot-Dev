//! The native side of `js/run.mjs`: same seed and step cap must print the same line.

use sha2::{Digest, Sha256};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let seed = args
        .next()
        .map_or(Ok(7), |text| text.parse())
        .map_err(|e| format!("seed: {e}"))?;
    let max_steps = args
        .next()
        .map_or(Ok(300), |text| text.parse())
        .map_err(|e| format!("steps: {e}"))?;
    let players = args
        .next()
        .map_or(Ok(8), |text| text.parse())
        .map_err(|e| format!("players: {e}"))?;
    let text = ti4_wasm::response(ti4_wasm::ti4_run_seeded(seed, players, max_steps))?;
    let result: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    println!(
        "decisions={} round={} sha256={}",
        result["decisions"],
        result["round"],
        hex::encode(Sha256::digest(text.as_bytes()))
    );
    Ok(())
}
