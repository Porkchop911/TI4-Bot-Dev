//! Replay already-extracted real decisions. These timings exclude feature extraction.
use super::*;
fn read_samples(path: &str) -> Vec<Sample> {
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    rows.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let index = s["row"].as_u64().unwrap() as usize;
            let row = FACTIONS
                .iter()
                .map(|f| FactionRow::of(f).unwrap())
                .find(|r| r.index() == index)
                .unwrap();
            let options = s["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| SparseOption {
                    columns: serde_json::from_value(o[0].clone()).unwrap(),
                    values: serde_json::from_value(o[1].clone()).unwrap(),
                })
                .collect();
            Sample {
                row,
                head: s["head"].as_str().unwrap().to_owned(),
                options,
            }
        })
        .collect()
}
fn timed_forward(actor: &Actor, samples: &[Sample], batch_size: usize, repeats: usize) -> Duration {
    let started = Instant::now();
    for _ in 0..repeats {
        for group in samples.chunks(batch_size) {
            if batch_size == 1 {
                let _ = std::hint::black_box(
                    actor
                        .logits(&group[0].options, &group[0].head, group[0].row)
                        .unwrap(),
                );
            } else {
                let mut options = Vec::new();
                let mut heads = Vec::new();
                let mut rows = Vec::new();
                for s in group {
                    let n = s.options.len();
                    options.extend(s.options.iter().cloned());
                    heads.extend(std::iter::repeat_n(
                        Actor::head_index(&s.head).unwrap() as i64,
                        n,
                    ));
                    rows.extend(std::iter::repeat_n(s.row.index() as i64, n));
                }
                let _ = std::hint::black_box(actor.logits_mixed(&options, &heads, &rows).unwrap());
            }
        }
    }
    started.elapsed()
}
pub fn run(actor: &Actor, path: &str) {
    let samples = read_samples(path);
    freeze_probe(&samples);
    allocation_probe(&samples);
    if flag("--allocation-only") {
        return;
    }
    let repeats: usize = number("--repeats", 5);
    println!(
        "MICRO samples={} repeats={repeats} input_bytes={}",
        samples.len(),
        actor.input().numel() * 4
    );
    // Same real sparse input, independent Tensor handles. Shared storage is immutable throughout.
    for threads in [1, 8, 32] {
        let team = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for shared in [false, true] {
            let start = Instant::now();
            let tables: Vec<_> = (0..threads)
                .map(|_| {
                    if shared {
                        actor.input().shallow_clone()
                    } else {
                        actor.input().copy()
                    }
                })
                .collect();
            let copy = start.elapsed();
            let chunks: Vec<_> = tables
                .into_iter()
                .zip(samples.chunks(samples.len().div_ceil(threads)))
                .collect();
            let start = Instant::now();
            let hashes: Vec<String> = team.install(|| {
                chunks
                    .into_par_iter()
                    .map(|(table, chunk)| {
                        let mut digest = Sha256::new();
                        for rep in 0..repeats {
                            for s in chunk {
                                let parts: Vec<_> = s
                                    .options
                                    .iter()
                                    .map(|o| (o.columns.as_slice(), o.values.as_slice()))
                                    .collect();
                                let out = ti4_tensor::gather_reduce_batch(&table, &parts).unwrap();
                                // Hash one pass identically in both storage modes; remaining passes time work.
                                if rep == 0 {
                                    for x in ti4_tensor::to_vec(&out).unwrap() {
                                        digest.update(x.to_bits().to_le_bytes());
                                    }
                                }
                                let _ = std::hint::black_box(out);
                            }
                        }
                        format!("{:x}", digest.finalize())
                    })
                    .collect()
            });
            println!(
                "TABLE threads={threads} shared={shared} copy_seconds={:.9} seconds={:.9} hashes={hashes:?}",
                copy.as_secs_f64(),
                start.elapsed().as_secs_f64()
            );
        }
    }
    for batch_size in [1, 3, 8, 32] {
        let elapsed = timed_forward(actor, &samples, batch_size, repeats);
        let mut changed = 0;
        let mut total = 0;
        let mut max_diff = 0.0f32;
        if batch_size > 1 {
            for group in samples.chunks(batch_size) {
                let mut options = Vec::new();
                let mut heads = Vec::new();
                let mut rows = Vec::new();
                let mut expected = Vec::new();
                for s in group {
                    expected.extend(
                        ti4_tensor::to_vec(&actor.logits(&s.options, &s.head, s.row).unwrap())
                            .unwrap(),
                    );
                    let n = s.options.len();
                    options.extend(s.options.iter().cloned());
                    heads.extend(std::iter::repeat_n(
                        Actor::head_index(&s.head).unwrap() as i64,
                        n,
                    ));
                    rows.extend(std::iter::repeat_n(s.row.index() as i64, n));
                }
                let actual =
                    ti4_tensor::to_vec(&actor.logits_mixed(&options, &heads, &rows).unwrap())
                        .unwrap();
                for (a, b) in actual.iter().zip(expected) {
                    total += 1;
                    if a.to_bits() != b.to_bits() {
                        changed += 1;
                    }
                    max_diff = max_diff.max((*a - b).abs());
                }
            }
        }
        println!(
            "BATCH decisions={batch_size} seconds={:.9} changed_logits={changed}/{total} max_abs_diff={max_diff}",
            elapsed.as_secs_f64()
        );
    }
}

// Original canonicalisation versus a strictly-increasing fast path. No changed sum order.
fn canonicalise(o: &mut SparseOption, fast: bool) {
    if fast && o.columns.windows(2).all(|p| p[0] < p[1]) {
        return;
    }
    let mut pairs: Vec<_> = o
        .columns
        .iter()
        .copied()
        .zip(o.values.iter().copied())
        .collect();
    pairs.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| ti4_tensor::total_order_key(a.1).cmp(&ti4_tensor::total_order_key(b.1)))
    });
    o.columns.clear();
    o.values.clear();
    for (c, v) in pairs {
        if o.columns.last() == Some(&c) {
            *o.values.last_mut().unwrap() += v;
        } else {
            o.columns.push(c);
            o.values.push(v);
        }
    }
}
fn freeze_probe(samples: &[Sample]) {
    let original: Vec<_> = samples
        .iter()
        .flat_map(|s| s.options.iter().cloned())
        .collect();
    for o in &original {
        let mut a = o.clone();
        let mut b = o.clone();
        canonicalise(&mut a, false);
        canonicalise(&mut b, true);
        assert_eq!(a.columns, b.columns);
        assert_eq!(
            a.values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            b.values.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
        );
    }
    for threads in [1, 32] {
        let team = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for pair in 0..3 {
            for fast in if pair % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                // Clone once, outside timing. The sampled corpus is already mostly canonical.
                let mut inputs = original.clone();
                let start = Instant::now();
                team.install(|| {
                    inputs
                        .par_chunks_mut(original.len().div_ceil(threads))
                        .for_each(|chunk| {
                            for _ in 0..20 {
                                for o in chunk.iter_mut() {
                                    canonicalise(o, fast);
                                }
                            }
                        })
                });
                println!(
                    "CANON threads={threads} pair={pair} fast={fast} seconds={:.9} options={} passes=20",
                    start.elapsed().as_secs_f64(),
                    original.len()
                );
            }
        }
    }
}

fn canonicalise_scratch(o: &mut SparseOption, pairs: &mut Vec<(i64, f32)>) {
    pairs.clear();
    pairs.extend(o.columns.iter().copied().zip(o.values.iter().copied()));
    pairs.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| ti4_tensor::total_order_key(a.1).cmp(&ti4_tensor::total_order_key(b.1)))
    });
    o.columns.clear();
    o.values.clear();
    for &(c, v) in pairs.iter() {
        if o.columns.last() == Some(&c) {
            *o.values.last_mut().unwrap() += v;
        } else {
            o.columns.push(c);
            o.values.push(v);
        }
    }
}
fn allocation_probe(samples: &[Sample]) {
    let original: Vec<_> = samples
        .iter()
        .flat_map(|s| s.options.iter().cloned())
        .collect();
    let capacity = original.iter().map(|o| o.columns.len()).max().unwrap();
    let mut scratch = Vec::with_capacity(capacity);
    for o in &original {
        let mut a = o.clone();
        let mut b = o.clone();
        canonicalise(&mut a, false);
        canonicalise_scratch(&mut b, &mut scratch);
        assert_eq!(a.columns, b.columns);
        assert_eq!(
            a.values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            b.values.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
        );
    }
    for threads in [1, 32] {
        let team = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for pair in 0..5 {
            for reuse in if pair % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let mut inputs = original.clone();
                let mut buffers: Vec<Vec<(i64, f32)>> =
                    (0..threads).map(|_| Vec::with_capacity(capacity)).collect();
                let start = Instant::now();
                team.install(|| {
                    inputs
                        .par_chunks_mut(original.len().div_ceil(threads))
                        .zip(buffers.par_iter_mut())
                        .for_each(|(chunk, scratch)| {
                            for _ in 0..100 {
                                for o in chunk.iter_mut() {
                                    if reuse {
                                        canonicalise_scratch(o, scratch);
                                    } else {
                                        canonicalise(o, false);
                                    }
                                }
                            }
                        })
                });
                println!(
                    "ALLOC threads={threads} pair={pair} reuse={reuse} seconds={:.9} options={} passes=100",
                    start.elapsed().as_secs_f64(),
                    original.len()
                );
            }
        }
    }
}
