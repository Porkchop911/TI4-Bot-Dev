//! What extra residual blocks would cost: a timing probe, not a training path.
//!
//! Replays captured real decisions (the `inference_cost --capture` format) through the current
//! trunk, and through the same trunk with `--blocks` extra width-sized residual blocks
//! `z + W_b·relu(W_a·z + b_a) + b_b` appended after it.
//!
//! - **CPU, one decision at a time**, as rollouts run: the policy trunk over the decision's options,
//!   one critic-sized trunk row (the shared critic goes through the same blocks), and the head
//!   readout.
//! - **CUDA forward and backward** over PPO-sized minibatches of `--minibatch` decisions, as the
//!   optimiser runs. Adam's step is left out: two blocks add 0.13M parameters to 5.3M.
//!
//! The block weights are random rather than zero, so no kernel can shortcut them; timing is all
//! this measures. Arms alternate pass by pass so drift lands on both; the median pass is reported.
//!
//! usage: trunk_depth_cost --bundle <dir> --samples <json> [--blocks 2] [--passes 5]
//!        [--minibatch 4096] [--steps 20]
use std::collections::BTreeMap;
use std::time::Instant;

use ti4_mlp::{Actor, FactionRow, SparseOption};
use ti4_tensor::{Device, Kind, Tensor};

struct Sample {
    row: FactionRow,
    head: i64,
    options: Vec<SparseOption>,
}

/// `(W_a, b_a, W_b, b_b)` per block.
type Block = (Tensor, Tensor, Tensor, Tensor);

fn argument(name: &str) -> Option<String> {
    let arguments: Vec<String> = std::env::args().collect();
    arguments.iter().position(|a| a == name).and_then(|i| arguments.get(i + 1).cloned())
}

fn number<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |v| v.parse().unwrap_or_else(|_| panic!("invalid {name}")))
}

fn read_samples(path: &str, bundle: &str) -> Vec<Sample> {
    // Faction rows by index, from the bundle's own roster.
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(bundle).join("manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    let rows: BTreeMap<usize, FactionRow> = manifest["factions"]
        .as_array()
        .expect("factions")
        .iter()
        .map(|alias| FactionRow::of(alias.as_str().expect("alias")).expect("roster faction"))
        .map(|row| (row.index(), row))
        .collect();
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("samples")).expect("samples JSON");
    data.as_array()
        .expect("sample array")
        .iter()
        .map(|s| {
            let head = Actor::head_index(Actor::resolve_head(s["head"].as_str().expect("head")))
                .expect("known head");
            Sample {
                row: rows[&usize::try_from(s["row"].as_u64().expect("row")).expect("row fits")],
                head: i64::try_from(head).expect("head fits"),
                options: s["options"]
                    .as_array()
                    .expect("options")
                    .iter()
                    .map(|o| SparseOption {
                        columns: serde_json::from_value(o[0].clone()).expect("columns"),
                        values: serde_json::from_value(o[1].clone()).expect("values"),
                    })
                    .collect(),
            }
        })
        .collect()
}

fn blocks(count: usize, width: i64, device: Device, grad: bool) -> Vec<Block> {
    let he = (2.0 / width as f64).sqrt();
    (0..count)
        .map(|_| {
            let t = |x: Tensor| x.set_requires_grad(grad);
            (
                t(Tensor::randn([width, width], (Kind::Float, device)) * he),
                t(Tensor::zeros([width], (Kind::Float, device))),
                t(Tensor::randn([width, width], (Kind::Float, device)) * 0.01),
                t(Tensor::zeros([width], (Kind::Float, device))),
            )
        })
        .collect()
}

fn apply(z: Tensor, blocks: &[Block]) -> Tensor {
    blocks.iter().fold(z, |z, (wa, ba, wb, bb)| {
        let inner = (z.matmul(&wa.tr()) + ba).relu();
        &z + inner.matmul(&wb.tr()) + bb
    })
}

/// One rollout decision: policy logits over the options plus the critic's single row.
fn decide(actor: &Actor, sample: &Sample, blocks: &[Block]) -> f64 {
    let f = i64::try_from(sample.row.index()).expect("row fits");
    let h = sample.head;
    let z = apply(actor.trunk(&sample.options, sample.row).expect("trunk"), blocks);
    let w = actor.shared_readout().get(h) + actor.delta().get(f).get(h);
    let b = actor.b_shared().get(h) + actor.b_delta().get(f).get(h);
    let logits = z.mv(&w) + b;
    let c = apply(actor.trunk(&sample.options[..1], sample.row).expect("critic trunk"), blocks);
    let v = c.mv(actor.value_readout()) + actor.b_value();
    logits.sum(Kind::Float).double_value(&[]) + v.sum(Kind::Float).double_value(&[])
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

fn cpu(actor: &Actor, samples: &[Sample], extra: &[Block], passes: usize) -> (f64, f64) {
    let arms: [&[Block]; 2] = [&[], extra];
    let mut times = [Vec::new(), Vec::new()];
    let mut sink = 0.0;
    for s in samples.iter().take(200) {
        sink += decide(actor, s, &[]) + decide(actor, s, extra);
    }
    for pass in 0..passes {
        for k in 0..2 {
            let arm = (pass + k) % 2;
            let start = Instant::now();
            for s in samples {
                sink += decide(actor, s, arms[arm]);
            }
            times[arm].push(start.elapsed().as_secs_f64() * 1e6 / samples.len() as f64);
        }
    }
    std::hint::black_box(sink);
    let [a, b] = times;
    (median(a), median(b))
}

struct Trainable {
    input: Tensor,
    embedding: Tensor,
    b1: Tensor,
    hidden: Tensor,
    b2: Tensor,
    w_shared: Tensor,
    b_shared: Tensor,
    delta: Tensor,
    b_delta: Tensor,
    value: Tensor,
    b_value: Tensor,
}

impl Trainable {
    fn of(actor: &Actor, device: Device) -> Self {
        let p = |t: &Tensor| t.to_device(device).detach().copy().set_requires_grad(true);
        Self {
            input: p(actor.input()),
            embedding: p(actor.embedding()),
            b1: p(actor.b1()),
            hidden: p(actor.hidden()),
            b2: p(actor.b2()),
            w_shared: p(actor.shared_readout()),
            b_shared: p(actor.b_shared()),
            delta: p(actor.delta()),
            b_delta: p(actor.b_delta()),
            value: p(actor.value_readout()),
            b_value: p(actor.b_value()),
        }
    }

    /// `trunk_mixed` from ti4-mlp, followed by the extra blocks.
    fn trunk(&self, batch: &[(&[i64], &[f32])], rows: &Tensor, extra: &[Block]) -> Tensor {
        let x = ti4_tensor::gather_reduce_batch(&self.input, batch).expect("gather");
        let identity = self.embedding.index_select(0, rows);
        let width = self.hidden.size()[0];
        let pad = Tensor::zeros(
            [identity.size()[0], width - identity.size()[1]],
            (Kind::Float, self.input.device()),
        );
        let identity = Tensor::cat(&[identity, pad], 1);
        let first = (x + identity + &self.b1).relu();
        apply((first.matmul(&self.hidden.tr()) + &self.b2).relu(), extra)
    }
}

/// One PPO minibatch's forward and backward, policy and shared critic, milliseconds.
fn step(net: &Trainable, samples: &[&Sample], extra: &[Block]) -> f64 {
    let device = net.input.device();
    let width = net.hidden.size()[0];
    let heads = net.w_shared.size()[0];
    let start = Instant::now();
    let mut batch = Vec::new();
    let mut rows = Vec::new();
    let mut head_of = Vec::new();
    let mut critic = Vec::new();
    let mut critic_rows = Vec::new();
    for s in samples {
        let f = i64::try_from(s.row.index()).expect("row fits");
        for o in &s.options {
            batch.push((o.columns.as_slice(), o.values.as_slice()));
            rows.push(f);
            head_of.push(s.head);
        }
        critic.push((s.options[0].columns.as_slice(), s.options[0].values.as_slice()));
        critic_rows.push(f);
    }
    let rows = Tensor::from_slice(&rows).to_device(device);
    let head_index = Tensor::from_slice(&head_of).to_device(device);
    let critic_rows = Tensor::from_slice(&critic_rows).to_device(device);

    let z = net.trunk(&batch, &rows, extra);
    let pair = &rows * heads + &head_index;
    let w = net.w_shared.index_select(0, &head_index)
        + net.delta.view([-1, width]).index_select(0, &pair);
    let b = net.b_shared.index_select(0, &head_index)
        + net.b_delta.view([-1]).index_select(0, &pair);
    let logits = (z * w).sum_dim_intlist([1i64].as_slice(), false, Kind::Float) + b;
    let c = net.trunk(&critic, &critic_rows, extra);
    let values = c.mv(&net.value) + &net.b_value;
    let loss = logits.mean(Kind::Float) + values.pow_tensor_scalar(2).mean(Kind::Float) * 0.5;
    loss.backward();
    // Reading a gradient waits for the whole backward pass on the device.
    std::hint::black_box(net.hidden.grad().sum(Kind::Float).double_value(&[]));
    start.elapsed().as_secs_f64() * 1e3
}

fn cuda(actor: &Actor, samples: &[Sample], count: usize, minibatch: usize, steps: usize, passes: usize) -> (f64, f64, f64) {
    let device = Device::Cuda(0);
    let net = Trainable::of(actor, device);
    let extra = blocks(count, actor.width(), device, true);
    let arms: [&[Block]; 2] = [&[], &extra];
    let batches: Vec<Vec<&Sample>> = (0..steps)
        .map(|i| (0..minibatch).map(|j| &samples[(i * minibatch + j) % samples.len()]).collect())
        .collect();
    let rows_per_batch = batches[0].iter().map(|s| s.options.len()).sum::<usize>() as f64;
    for batch in batches.iter().take(3) {
        step(&net, batch, arms[0]);
        step(&net, batch, arms[1]);
    }
    let mut times = [Vec::new(), Vec::new()];
    for pass in 0..passes {
        for k in 0..2 {
            let arm = (pass + k) % 2;
            let total: f64 = batches.iter().map(|batch| step(&net, batch, arms[arm])).sum();
            times[arm].push(total / steps as f64);
        }
    }
    let [a, b] = times;
    (median(a), median(b), rows_per_batch)
}

fn main() {
    ti4_tensor::configure_deterministic(20_260_911).expect("backend");
    let bundle = argument("--bundle").expect("--bundle <dir>");
    let path = argument("--samples").expect("--samples <json>");
    let count: usize = number("--blocks", 2);
    let passes: usize = number("--passes", 5);
    let minibatch: usize = number("--minibatch", 4096);
    let steps: usize = number("--steps", 20);

    let actor = ti4_mlp::bundle::read(std::path::Path::new(&bundle)).expect("bundle").actor;
    let samples = read_samples(&path, &bundle);
    let options: usize = samples.iter().map(|s| s.options.len()).sum();
    println!(
        "bundle {bundle}\nsamples {} decisions, {:.2} options/decision, width {}, extra blocks {count}",
        samples.len(),
        options as f64 / samples.len() as f64,
        actor.width()
    );
    println!("actor tensors require grad: {}", actor.hidden().requires_grad());

    let extra = blocks(count, actor.width(), Device::Cpu, false);
    let (base, deeper) = cpu(&actor, &samples, &extra, passes);
    println!(
        "CPU rollout path   {base:8.2} us/decision  ->  {deeper:8.2} us/decision   ({:+.2} us, {:+.1}%)",
        deeper - base,
        100.0 * (deeper / base - 1.0)
    );

    if Device::cuda_if_available().is_cuda() {
        let (base, deeper, rows) = cuda(&actor, &samples, count, minibatch, steps, passes);
        println!(
            "CUDA minibatch     {base:8.2} ms  ->  {deeper:8.2} ms   ({:+.2} ms, {:+.1}%)   [{minibatch} decisions, {rows:.0} option rows, fwd+bwd]",
            deeper - base,
            100.0 * (deeper / base - 1.0)
        );
    } else {
        println!("CUDA not available in this build; optimiser side not measured");
    }
}
