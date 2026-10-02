//! Rebuild checked-in fixtures from the current versioned protocol producers.
//! `cargo run -p ti4-server --example generate_protocol_fixtures -- --write`
use std::path::Path;

use ti4_server::fixtures::{
    sample_actor_snapshot, sample_opponent_snapshot, sample_spectator_snapshot,
    sample_stale_submission_rejected, sample_terminal_game_over,
};

fn main() {
    let write = std::env::args().skip(1).eq(["--write".to_owned()]);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    for (name, message) in [
        ("actor_snapshot.json", sample_actor_snapshot()),
        ("opponent_snapshot.json", sample_opponent_snapshot()),
        ("spectator_snapshot.json", sample_spectator_snapshot()),
        (
            "stale_submission_rejected.json",
            sample_stale_submission_rejected(),
        ),
        ("terminal_game_over.json", sample_terminal_game_over()),
    ] {
        let mut expected = serde_json::to_string_pretty(&message).expect("fixture serialization");
        expected.push('\n');
        let path = root.join(name);
        if write {
            std::fs::write(&path, expected).expect("write fixture");
        } else {
            assert_eq!(
                std::fs::read_to_string(&path).expect("read fixture"),
                expected,
                "{name}"
            );
        }
    }
}
