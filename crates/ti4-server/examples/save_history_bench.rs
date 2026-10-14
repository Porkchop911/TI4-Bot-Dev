//! Times FileGameStore::save_history / load_history on a recorded game directory.
//! usage: save_history_bench <game dir with history.json> <scratch base dir> [rounds]
use std::time::Instant;
use ti4_server::storage::FileGameStore;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = std::path::PathBuf::from(&args[1]);
    let base = std::path::PathBuf::from(&args[2]);
    let rounds: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(5);
    let game_id = src.file_name().unwrap().to_string_lossy().to_string();
    let dst = base.join(&game_id);
    std::fs::create_dir_all(&dst).unwrap();
    for file in ["history.json", "init.json"] {
        let _ = std::fs::copy(src.join(file), dst.join(file));
    }
    let store = FileGameStore::new(&base).unwrap();
    let started = Instant::now();
    let history = store.load_history(&game_id).unwrap().expect("history");
    println!(
        "load_history: {:?} ({} decisions, {} events)",
        started.elapsed(),
        history.decisions.len(),
        history.events.len()
    );
    for _ in 0..rounds {
        let cloned = Instant::now();
        let copy = history.clone();
        let clone_time = cloned.elapsed();
        let started = Instant::now();
        store.save_history(&game_id, &copy).unwrap();
        println!(
            "clone {:?}  save_history {:?}",
            clone_time,
            started.elapsed()
        );
    }
}
