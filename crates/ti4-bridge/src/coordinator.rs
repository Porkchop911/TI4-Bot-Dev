//! Runtime acknowledgement gate between authoritative Rust transitions and TTS.

use std::time::{Duration, Instant};

use crate::{
    client::{BridgeClient, ClientError},
    wire::{Command, CommandId, Outcome, OutcomeError, OutcomeStatus},
};

/// Executes already-validated commands serially and requires an explicit successful result.
#[derive(Debug, Clone)]
pub struct Coordinator {
    client: BridgeClient,
    timeout: Duration,
    poll_interval: Duration,
}

impl Coordinator {
    #[must_use]
    pub fn new(client: BridgeClient) -> Self {
        Self {
            client,
            timeout: Duration::from_secs(10),
            poll_interval: Duration::from_millis(200),
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Queue each command only after its predecessor has reported success.
    ///
    /// # Errors
    /// Stops on transport failure, malformed result lines, refusal, executor error, or silence.
    pub fn execute(&self, commands: &[Command]) -> Result<Vec<Outcome>, CoordinatorError> {
        let mut outcomes = Vec::with_capacity(commands.len());
        for command in commands {
            let queued = self.client.queue(command)?;
            let outcome = self.wait_for(queued.id)?;
            if outcome.status != OutcomeStatus::Ok {
                return Err(CoordinatorError::CommandFailed(outcome));
            }
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    fn wait_for(&self, id: CommandId) -> Result<Outcome, CoordinatorError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            for line in self.client.log()? {
                match Outcome::parse(&line) {
                    Ok(outcome) if outcome.id == id => return Ok(outcome),
                    Ok(_) | Err(OutcomeError::NotAResult) => {}
                    Err(error) => return Err(CoordinatorError::MalformedOutcome(error)),
                }
            }
            if Instant::now() >= deadline {
                return Err(CoordinatorError::Timeout {
                    id,
                    waited: self.timeout,
                });
            }
            std::thread::sleep(self.poll_interval);
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("executor result could not be parsed: {0}")]
    MalformedOutcome(OutcomeError),
    #[error("command {0:?} did not succeed")]
    CommandFailed(Outcome),
    #[error("command {id} produced no result within {waited:?}")]
    Timeout { id: CommandId, waited: Duration },
}
