//! The rule bot as an engine [`Decider`], without a session around it: for headless benchmarks
//! and the replayer, which seat deciders directly on an engine `Table`.

use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};

/// Same picks as a `smart` seat ([`crate::choose`]), no think delay. `seat` is the seat's place
/// in the game's seat order, from 0. The decision index is this seat's own count rather than the
/// game's, so it is deterministic per (seed, seat) but not stream-identical to a server game.
pub struct RuleBotDecider {
    seat: usize,
    game_seed: u64,
    answered: usize,
}

impl RuleBotDecider {
    #[must_use]
    pub const fn new(seat: usize, game_seed: u64) -> Self {
        Self {
            seat,
            game_seed,
            answered: 0,
        }
    }

    fn answer(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        let (picked, _) = crate::choose(choice, seen, self.game_seed, self.seat, self.answered);
        self.answered += 1;
        Ok(choice.options[picked].clone())
    }
}

impl Decider for RuleBotDecider {
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
