//! Game loop stub.

use ti4_model::*;

pub struct GameLoop {
    pub game: GameState,
    pub running: bool,
}

impl GameLoop {
    pub fn new(game: GameState) -> Self {
        Self { game, running: false }
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn step(&mut self) -> Result<bool, anyhow::Error> {
        todo!("M04-M06: implement game loop step")
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}
