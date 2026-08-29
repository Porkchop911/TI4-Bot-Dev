//! Recompute the behavioural bounds under the current tree, for a versioned re-baseline.
//!
//! Prints old and new side by side so the pair can be recorded in `plans/evidence/M08-021.md`,
//! which the re-baseline discipline in `crate::behavior` requires. It changes nothing: moving the
//! bounds is a source edit somebody reviews, not something a tool does.
//!
//! Uses `recompute_bound`, the same derivation the recorded values came from — the bootstrap draws
//! and seed are constants in `behavior`, and changing either is itself a re-baseline event.

use ti4_sim::behavior;

fn main() {
    let batch = behavior::play_batch(ti4_content::ContentStore::embedded());
    let old = behavior::baseline_bounds();
    let metrics = behavior::batch_metrics(&batch);

    println!("metric                          old lo       old hi   |   new lo       new hi   | now");
    let mut moved = 0usize;
    for (name, (lo, hi)) in &old {
        let Some((new_lo, new_hi)) = behavior::recompute_bound(&batch, name) else {
            println!("{name:<28} (no derivation)");
            continue;
        };
        let now = metrics.get(name).copied().unwrap_or(f64::NAN);
        let inside = now >= *lo && now <= *hi;
        if !inside {
            moved += 1;
        }
        println!(
            "{name:<28} {lo:>10.6} {hi:>12.6}   | {new_lo:?} {new_hi:?} | {now:>10.6}{}",
            if inside { "" } else { "  <-- outside" }
        );
    }
    println!();
    println!("{moved} metric(s) outside the recorded bounds");
}
