//! The in-process random bot: a seat answered inside the game worker by a decider that picks
//! among the engine's legal options.
//!
//! It is the server-side counterpart of the nightly browser harness's `random` policy
//! (`web/e2e/smokePlaythrough.ts`, `smokePolicy.ts`) without the browser. The harness clicks a
//! random enabled control and relies on helpers to finish staged workflows (confirm a complete
//! payment, confirm hits, confirm command tokens, propose a listed trade, end the turn). The
//! engine already collapses each of those workflows into one question whose options are the
//! legal answers, so a pick among the options is the same behaviour one level down. The
//! harness's special cases that the option list still needs are mirrored in [`weight`]. The same
//! decider plays the rule bot and the MLP seats (`ti4_host::bots`).
//!
//! Determinism: every pick is seeded from (game seed, seat place, decision index), where the seat
//! place is the index of the seat in the seat order of the game (`GameState::players`, from 0)
//! and the decision index is the number of decisions answered before this one in the whole game
//! (replayed ones included). The same position therefore gets the same answer, in the same
//! process or after a restart, and the same table of a seed is the same game in the page: the ids
//! of the seats (random on the server) play no part. A bot only ever *produces* new decisions; a
//! replay or recovery feeds the recorded answers through the replaying layer above this decider,
//! which gives each of them to the seat's bot ([`SharedBot`], `Bot::follow`), so that an MLP seat
//! keeps what the decision made it keep.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};

use crate::session::worker::SessionShared;

pub use ti4_rule_bot::random::{decision_rng, pick_index, weight};

/// Environment variable with the think delay between bot decisions, in milliseconds.
pub const RANDOM_BOT_DELAY_ENV: &str = "TI4_RANDOM_BOT_DELAY_MS";
/// Default think delay: long enough to follow along, short enough not to drag.
pub const DEFAULT_RANDOM_BOT_DELAY_MS: u64 = 600;
/// Hard cap so a typo cannot park a game for minutes per decision.
const MAX_DELAY_MS: u64 = crate::session::table::MAX_BOT_DELAY_MS;

/// Process-wide override of the think delay (tests and soaks); `u64::MAX` means none.
static DELAY_OVERRIDE: AtomicU64 = AtomicU64::new(u64::MAX);

/// Override the think delay for every session started afterwards in this process, ahead of the
/// environment variable. For tests and soaks that recover games from disk and so cannot pass a
/// per-session value.
pub fn set_random_bot_delay_override(delay_ms: Option<u64>) {
    DELAY_OVERRIDE.store(delay_ms.unwrap_or(u64::MAX), Ordering::SeqCst);
}

/// The configured think delay: the process override, else `TI4_RANDOM_BOT_DELAY_MS`, else
/// 600 ms. `0` disables it.
#[must_use]
pub fn random_bot_delay_ms() -> u64 {
    let forced = DELAY_OVERRIDE.load(Ordering::SeqCst);
    if forced != u64::MAX {
        return forced.min(MAX_DELAY_MS);
    }
    std::env::var(RANDOM_BOT_DELAY_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_RANDOM_BOT_DELAY_MS)
        .min(MAX_DELAY_MS)
}

/// Shared count of decisions answered so far in this game, replayed ones included. Advanced by
/// [`CountingDecider`], read by [`RandomBotDecider`] as the decision index.
pub type DecisionCounter = Arc<AtomicUsize>;

/// Counts every answered decision of the seat it wraps (placed outside the replaying layer, so
/// recorded answers count too).
pub struct CountingDecider {
    pub inner: Box<dyn Decider>,
    pub counter: DecisionCounter,
}

impl Decider for CountingDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.inner.choose(choice)?;
        self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(option)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.inner.choose_seeing(choice, seen)?;
        self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(option)
    }
}

/// The seat of a bot of `ti4_host::bots`, shared by its decider and the replay of the session
/// (which gives it the recorded decisions of the seat: `Bot::follow`).
pub type SharedBot = Arc<Mutex<ti4_host::bots::Bot>>;

/// The decider of a `SeatController::Bot` seat: [`ti4_host::bots::choose`] on its bot, then the
/// think time of the table.
pub struct RandomBotDecider {
    /// The place of the seat in the seat order of the game, from 0.
    place: usize,
    game_seed: u64,
    counter: DecisionCounter,
    shared: Arc<Mutex<SessionShared>>,
    bot: SharedBot,
}

impl RandomBotDecider {
    /// A random seat.
    #[must_use]
    pub fn new(
        place: usize,
        game_seed: u64,
        counter: DecisionCounter,
        shared: Arc<Mutex<SessionShared>>,
    ) -> Self {
        Self {
            place,
            game_seed,
            counter,
            shared,
            bot: Arc::new(Mutex::new(ti4_host::bots::Bot::Random)),
        }
    }

    /// The same seat played by `bot`.
    #[must_use]
    pub fn playing(mut self, bot: SharedBot) -> Self {
        self.bot = bot;
        self
    }

    /// Wait while a history change is running, then hand the answer over. `Err` when stopped.
    fn release(&self) -> Result<(), ()> {
        loop {
            {
                let mut lock = self.shared.lock().expect("shared lock");
                if lock.stopped {
                    lock.bot_deciding = false;
                    return Err(());
                }
                if !lock.history_pause {
                    lock.bot_deciding = false;
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Wait out the think time in short slices without holding the session lock, so a stop
    /// (undo, redo, shutdown) is noticed within a slice. `Err` when the session was stopped.
    ///
    /// The wait is the value of the table as it is now ([`SessionShared::bot_delay_ms`]): the
    /// host can change it while the game runs. A reaction window that the seat only declines
    /// does not wait (owner, 2026-10-12): a table of eight has many of them for each action, and
    /// nothing is there to follow. While the seat waits, the other seats are told who the game
    /// waits for, as for a person.
    fn think(&self, choice: &Choice, picked: usize, rng: &mut ChaCha8Rng) -> Result<(), ()> {
        {
            let mut shared = self.shared.lock().expect("shared lock");
            let window = ti4_host::projection::is_reaction_window(choice);
            if shared.bot_delay_ms == 0 || (window && choice.options[picked].is_decline()) {
                return Ok(());
            }
            shared.broadcast_waiting_for(choice);
        }
        let jitter = rng.random_range(0.6..1.4);
        let started = std::time::Instant::now();
        loop {
            // Read in every slice: a wait that the host shortens ends this wait too.
            let delay_ms = {
                let shared = self.shared.lock().expect("shared lock");
                if shared.stopped {
                    return Err(());
                }
                shared.bot_delay_ms
            };
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wait = Duration::from_millis((delay_ms as f64 * jitter) as u64);
            let Some(remaining) = wait.checked_sub(started.elapsed()) else {
                return Ok(());
            };
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
        }
    }
}

impl RandomBotDecider {
    fn answer(
        &self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        let index = self.counter.load(Ordering::SeqCst);
        let (picked, mut rng) = ti4_host::bots::choose(
            &mut self.bot.lock().expect("bot lock"),
            choice,
            seen,
            self.game_seed,
            self.place,
            index,
        );
        self.shared.lock().expect("shared lock").bot_deciding = true;
        // `release` clears `bot_deciding` under the same lock that checks the pause, so a
        // history change sees either a bot still holding its answer or the answer in the game.
        let released = if self.think(choice, picked, &mut rng).is_ok() {
            self.release()
        } else {
            self.shared.lock().expect("shared lock").bot_deciding = false;
            Err(())
        };
        if released.is_err() {
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "session was stopped".to_owned(),
            });
        }
        Ok(choice.options[picked].clone())
    }
}

impl Decider for RandomBotDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, None)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, Some(seen))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_model::id::PlayerId;
    use ti4_model::state::Phase;

    use super::*;

    fn decider(delay_ms: u64) -> (RandomBotDecider, Arc<Mutex<SessionShared>>) {
        let mut shared =
            SessionShared::new("wait".to_owned(), crate::fixtures::create_sample_game());
        shared.bot_delay_ms = delay_ms;
        let shared = Arc::new(Mutex::new(shared));
        (
            RandomBotDecider::new(0, 7, Arc::new(AtomicUsize::new(0)), shared.clone()),
            shared,
        )
    }

    /// A question with one answer, "Decline", so the pick is known.
    fn declined(source: DecisionSource, subtype: &str) -> Choice {
        let seat = PlayerId::new("p1");
        let mut choice = Choice::new(seat.clone(), "Play a card?", vec![ChoiceOption::decline()]);
        choice.context = Some(DecisionContext::new(
            seat,
            source,
            subtype,
            Phase::Action,
            1,
        ));
        choice
    }

    #[test]
    fn a_reaction_window_that_the_seat_only_declines_does_not_wait() {
        let (mut bot, _) = decider(400);
        let window = declined(
            DecisionSource::Reaction("sabotage".to_owned()),
            "reaction_window",
        );
        let started = Instant::now();
        assert!(bot.choose(&window).expect("an answer").is_decline());
        assert!(
            started.elapsed() < Duration::from_millis(150),
            "a declined window waited {:?}",
            started.elapsed()
        );

        // The same answer to a question of the game itself waits: 400 ms, jittered 0.6 to 1.4.
        let question = declined(DecisionSource::Rule("52.3".to_owned()), "secondary");
        let started = Instant::now();
        assert!(bot.choose(&question).expect("an answer").is_decline());
        assert!(
            started.elapsed() >= Duration::from_millis(200),
            "a question waited only {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_wait_that_is_shortened_ends_the_wait_of_the_seat_that_waits() {
        let (mut bot, shared) = decider(MAX_DELAY_MS);
        let question = declined(DecisionSource::Rule("52.3".to_owned()), "secondary");
        let started = Instant::now();
        let waiting = std::thread::spawn(move || bot.choose(&question).map(|option| option.id));
        std::thread::sleep(Duration::from_millis(60));
        assert!(!waiting.is_finished(), "the seat answered at once");
        assert!(shared.lock().expect("shared lock").bot_deciding);
        shared.lock().expect("shared lock").bot_delay_ms = 0;
        assert!(waiting.join().expect("the seat").is_ok());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!shared.lock().expect("shared lock").bot_deciding);
    }
}
