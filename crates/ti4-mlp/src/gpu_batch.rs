//! Experimental cross-game batched inference for bounded PPO diagnostics.
//!
//! The actor lives on exactly one service thread because tensors are `Send` but not `Sync`.
//! Rollout workers synchronously submit already-projected requests, preserving the engine's
//! synchronous `Decider` contract while allowing independent games to share CUDA launches.

use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ti4_tensor::to_vec;

use crate::{Actor, CriticInput, FactionRow, HeadLayout, SparseOption};

/// Aggregate service accounting for one frozen rollout wave.
#[derive(Debug, Clone, Copy, Default)]
pub struct GpuBatchStats {
    /// Requests answered by the service.
    pub requests: u64,
    /// Actor batches executed.
    pub batches: u64,
    /// Policy options scored.
    pub options: u64,
    /// Critic positions scored.
    pub critics: u64,
    /// Largest number of independent decisions in an actor batch.
    pub max_batch: u64,
    /// Sum of request residence time before the service starts a batch.
    pub queue_nanos: u128,
    /// Longest request residence time before the service starts a batch.
    pub max_queue_nanos: u128,
    /// Host packing from sparse requests into one actor call.
    pub pack_nanos: u128,
    /// Sum of service-side actor/critic scoring spans.
    pub score_nanos: u128,
}

#[derive(Debug)]
struct Request {
    options: Vec<SparseOption>,
    head: usize,
    row: FactionRow,
    temperature: f64,
    critic: Option<CriticInput>,
    sent: Instant,
    reply: mpsc::SyncSender<Result<Response, String>>,
}

#[derive(Debug)]
struct Response {
    probabilities: Vec<f64>,
    value: Option<f64>,
}

enum Message {
    Score(Request),
    Shutdown,
}

/// A cloneable handle used only by synchronous rollout deciders.
#[derive(Clone)]
pub struct GpuInferenceClient {
    sender: mpsc::SyncSender<Message>,
    layout: HeadLayout,
}

impl GpuInferenceClient {
    /// Submit one independent decision and wait for its actor and optional critic result.
    pub fn score(
        &self,
        options: Vec<SparseOption>,
        head: usize,
        row: FactionRow,
        temperature: f64,
        critic: Option<CriticInput>,
    ) -> Result<(Vec<f64>, Option<f64>), String> {
        if options.is_empty() {
            return Err("GPU inference was asked to score no legal options".to_owned());
        }
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(format!(
                "GPU inference temperature {temperature} is not positive and finite"
            ));
        }
        let (reply, response) = mpsc::sync_channel(0);
        self.sender
            .send(Message::Score(Request {
                options,
                head,
                row,
                temperature,
                critic,
                sent: Instant::now(),
                reply,
            }))
            .map_err(|_| "GPU inference service stopped before accepting a request".to_owned())?;
        let response = response
            .recv()
            .map_err(|_| "GPU inference service stopped before replying".to_owned())??;
        Ok((response.probabilities, response.value))
    }

    /// Resolve a requested head against the frozen service model's stored layout.
    #[must_use]
    pub fn resolve_layout_head<'a>(&self, requested: &'a str) -> &'a str {
        if self.layout.heads().contains(&requested) {
            requested
        } else {
            "other"
        }
    }

    /// Resolve a stored-layout head to the service model's readout index.
    pub fn layout_head_index(&self, name: &str) -> Result<usize, String> {
        self.layout
            .heads()
            .iter()
            .position(|known| *known == name)
            .ok_or_else(|| format!("GPU service layout has no head {name}"))
    }
}

/// Owns the service thread and must be shut down after every rollout wave.
pub struct GpuInferenceService {
    client: GpuInferenceClient,
    join: Option<JoinHandle<Result<(), String>>>,
    stats: Arc<Mutex<GpuBatchStats>>,
}

impl GpuInferenceService {
    /// Start a bounded actor service over a frozen actor already placed on CUDA.
    pub fn spawn(actor: Actor, batch_size: usize, flush: Duration) -> Result<Self, String> {
        if actor.battle_predictor().is_some() {
            // Seats on this path never see the actor, so they could not emit battle facts and an
            // arena bundle would silently play as the baseline.
            return Err("GPU batched inference does not support arena-capable bundles".to_owned());
        }
        if !(2..=128).contains(&batch_size) {
            return Err(format!("GPU batch size {batch_size} is outside 2..=128"));
        }
        if flush.is_zero() || flush > Duration::from_millis(20) {
            return Err("GPU partial-batch flush must be 1..=20 ms".to_owned());
        }
        let layout = actor.head_layout();
        let queue = batch_size
            .checked_mul(4)
            .ok_or_else(|| "GPU request queue capacity overflow".to_owned())?;
        let (sender, receiver) = mpsc::sync_channel(queue);
        let stats = Arc::new(Mutex::new(GpuBatchStats::default()));
        let service_stats = Arc::clone(&stats);
        let join = thread::Builder::new()
            .name("ti4-gpu-inference".to_owned())
            .spawn(move || serve(actor, receiver, batch_size, flush, service_stats))
            .map_err(|error| format!("starting GPU inference service: {error}"))?;
        Ok(Self {
            client: GpuInferenceClient { sender, layout },
            join: Some(join),
            stats,
        })
    }

    /// A client for one rollout worker or one MLP seat.
    #[must_use]
    pub fn client(&self) -> GpuInferenceClient {
        self.client.clone()
    }

    /// Flush pending work, stop the owner thread, and return its complete accounting.
    pub fn shutdown(mut self) -> Result<GpuBatchStats, String> {
        self.client
            .sender
            .send(Message::Shutdown)
            .map_err(|_| "GPU inference service stopped before shutdown".to_owned())?;
        let join = self
            .join
            .take()
            .ok_or_else(|| "GPU inference service was already shut down".to_owned())?;
        join.join()
            .map_err(|_| "GPU inference service panicked".to_owned())??;
        self.stats
            .lock()
            .map(|stats| *stats)
            .map_err(|_| "GPU inference service statistics lock was poisoned".to_owned())
    }
}

fn serve(
    actor: Actor,
    receiver: mpsc::Receiver<Message>,
    batch_size: usize,
    flush: Duration,
    stats: Arc<Mutex<GpuBatchStats>>,
) -> Result<(), String> {
    loop {
        let first = match receiver.recv() {
            Ok(Message::Score(request)) => request,
            Ok(Message::Shutdown) | Err(_) => return Ok(()),
        };
        let mut requests = vec![first];
        let deadline = Instant::now() + flush;
        let mut stopping = false;
        while requests.len() < batch_size {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            match receiver.recv_timeout(remaining) {
                Ok(Message::Score(request)) => requests.push(request),
                Ok(Message::Shutdown) => {
                    stopping = true;
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    stopping = true;
                    break;
                }
            }
        }
        score_batch(&actor, &mut requests, &stats);
        if stopping {
            return Ok(());
        }
    }
}

fn score_batch(actor: &Actor, requests: &mut [Request], stats: &Mutex<GpuBatchStats>) {
    let queued_at = Instant::now();
    let mut options = Vec::new();
    let mut heads = Vec::new();
    let mut rows = Vec::new();
    let mut ends = Vec::with_capacity(requests.len());
    for request in requests.iter() {
        let head = match i64::try_from(request.head) {
            Ok(head) => head,
            Err(_) => {
                reply_all(
                    requests,
                    "GPU inference head index does not fit i64".to_owned(),
                );
                return;
            }
        };
        let row = match i64::try_from(request.row.index()) {
            Ok(row) => row,
            Err(_) => {
                reply_all(
                    requests,
                    "GPU inference faction row does not fit i64".to_owned(),
                );
                return;
            }
        };
        let length = request.options.len();
        options.extend(request.options.iter().cloned());
        heads.extend(std::iter::repeat_n(head, length));
        rows.extend(std::iter::repeat_n(row, length));
        ends.push(options.len());
    }
    let scored_at = Instant::now();
    let result = tch::no_grad(|| -> Result<_, String> {
        let logits = to_vec(
            &actor
                .logits_mixed(&options, &heads, &rows)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("copying GPU actor logits: {e}"))?;
        let mut probabilities = Vec::with_capacity(requests.len());
        let mut start = 0usize;
        for (index, end) in ends.iter().enumerate() {
            probabilities.push(softmax(&logits[start..*end], requests[index].temperature)?);
            start = *end;
        }
        let critic_indices: Vec<usize> = requests
            .iter()
            .enumerate()
            .filter_map(|(index, request)| request.critic.as_ref().map(|_| index))
            .collect();
        let mut values = vec![None; requests.len()];
        if !critic_indices.is_empty() {
            let critics: Vec<&CriticInput> = critic_indices
                .iter()
                .filter_map(|index| requests[*index].critic.as_ref())
                .collect();
            let critic_rows: Vec<i64> = critic_indices
                .iter()
                .map(|index| i64::try_from(requests[*index].row.index()))
                .collect::<Result<_, _>>()
                .map_err(|_| "GPU critic faction row does not fit i64".to_owned())?;
            let scored = to_vec(
                &actor
                    .value_batch(&critics, &critic_rows)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("copying GPU critic values: {e}"))?;
            if scored.len() != critic_indices.len() {
                return Err(format!(
                    "GPU critic returned {} values for {} requests",
                    scored.len(),
                    critic_indices.len()
                ));
            }
            for (index, value) in critic_indices.into_iter().zip(scored) {
                let value = f64::from(value);
                if !value.is_finite() {
                    return Err("GPU critic returned a non-finite value".to_owned());
                }
                values[index] = Some(value);
            }
        }
        Ok((probabilities, values))
    });
    let finished_at = Instant::now();
    if let Ok(mut total) = stats.lock() {
        total.requests += u64::try_from(requests.len()).unwrap_or(u64::MAX);
        total.batches += 1;
        total.options += u64::try_from(options.len()).unwrap_or(u64::MAX);
        total.critics += u64::try_from(
            requests
                .iter()
                .filter(|request| request.critic.is_some())
                .count(),
        )
        .unwrap_or(u64::MAX);
        total.max_batch = total
            .max_batch
            .max(u64::try_from(requests.len()).unwrap_or(u64::MAX));
        total.pack_nanos += (scored_at - queued_at).as_nanos();
        total.score_nanos += (finished_at - scored_at).as_nanos();
        for request in requests.iter() {
            let queue = queued_at.duration_since(request.sent).as_nanos();
            total.queue_nanos += queue;
            total.max_queue_nanos = total.max_queue_nanos.max(queue);
        }
    }
    match result {
        Ok((probabilities, values)) => {
            for ((request, probabilities), value) in
                requests.iter_mut().zip(probabilities).zip(values)
            {
                let _ = request.reply.send(Ok(Response {
                    probabilities,
                    value,
                }));
            }
        }
        Err(error) => reply_all(requests, error),
    }
}

fn reply_all(requests: &mut [Request], error: String) {
    for request in requests {
        let _ = request.reply.send(Err(error.clone()));
    }
}

fn softmax(logits: &[f32], temperature: f64) -> Result<Vec<f64>, String> {
    if logits.is_empty() {
        return Err("GPU actor returned an empty decision".to_owned());
    }
    if logits.iter().any(|logit| !logit.is_finite()) {
        return Err("GPU actor returned a non-finite logit".to_owned());
    }
    // Match `Actor::probabilities`: f32 division before f32 max/subtraction, then f64 exp.
    let scores: Vec<f32> = logits
        .iter()
        .map(|logit| *logit / temperature as f32)
        .collect();
    if scores.iter().any(|score| !score.is_finite()) {
        return Err("GPU actor returned a non-finite temperature-scaled logit".to_owned());
    }
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let weights: Vec<f64> = scores
        .iter()
        .map(|score| f64::from(*score - max).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return Err("GPU actor produced an unusable softmax normalizer".to_owned());
    }
    let probabilities: Vec<f64> = weights.into_iter().map(|weight| weight / total).collect();
    if probabilities
        .iter()
        .any(|probability| !probability.is_finite() || *probability < 0.0)
    {
        return Err("GPU actor produced a malformed probability distribution".to_owned());
    }
    Ok(probabilities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Width;

    #[test]
    fn cpu_service_matches_cpu_actor_probabilities_and_critic_at_training_temperature() {
        let mut actor = Actor::zeros(Width::W128, 64);
        let _ = actor.input_mut().get(1).fill_(1.0);
        let _ = actor.input_mut().get(2).fill_(2.0);
        let _ = actor.b1_mut().fill_(0.25);
        let _ = actor.hidden_mut().fill_(0.01);
        let _ = actor.b2_mut().fill_(0.1);
        let _ = actor.shared_readout_mut().fill_(0.02);
        let _ = actor.value_readout_mut().fill_(0.03);
        let row = FactionRow::of("sol").expect("row");
        let options = vec![
            SparseOption {
                columns: vec![1],
                values: vec![1.0],
            },
            SparseOption {
                columns: vec![2],
                values: vec![1.0],
            },
        ];
        let critic = CriticInput::from_sparse(SparseOption {
            columns: vec![1, 2],
            values: vec![0.5, 1.0],
        });
        let head = actor.head_names()[0];
        let expected_probabilities = actor
            .probabilities(&options, head, row, 2.5)
            .expect("actor probabilities");
        let expected_value = actor.value(&critic, row).expect("actor value");
        let head_index = actor.layout_head_index(head).expect("head");
        let service =
            GpuInferenceService::spawn(actor.inference_copy(), 2, Duration::from_millis(2))
                .expect("starts");
        let client = service.client();
        let (actual_probabilities, actual_value) = client
            .score(options, head_index, row, 2.5, Some(critic))
            .expect("score");
        for (actual, expected) in actual_probabilities.iter().zip(expected_probabilities) {
            assert!(
                (actual - expected).abs() < 2e-4,
                "{actual} against {expected}"
            );
        }
        assert!(
            (actual_value.expect("critic value") - expected_value).abs() < 2e-4,
            "service critic differs from actor"
        );
        let stats = service.shutdown().expect("stops");
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.options, 2);
        assert_eq!(stats.batches, 1);
        assert_eq!(stats.critics, 1);
        assert_eq!(stats.max_batch, 1);
    }

    #[test]
    fn singleton_request_flushes_without_waiting_for_a_full_batch() {
        let service =
            GpuInferenceService::spawn(Actor::zeros(Width::W128, 64), 32, Duration::from_millis(1))
                .expect("starts");
        let (probabilities, value) = service
            .client()
            .score(
                vec![SparseOption {
                    columns: vec![1],
                    values: vec![1.0],
                }],
                0,
                FactionRow::of("sol").expect("row"),
                2.5,
                None,
            )
            .expect("partial batch returns");
        assert_eq!(probabilities, vec![1.0]);
        assert!(value.is_none());
        let stats = service.shutdown().expect("stops");
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.batches, 1);
        assert_eq!(stats.max_batch, 1);
    }

    #[test]
    fn softmax_refuses_nonfinite_logits() {
        assert!(softmax(&[f32::NAN], 1.0).is_err());
        assert!(softmax(&[], 1.0).is_err());
    }
}
