//! TTS command stub.

pub struct TtsCommands;

impl Default for TtsCommands {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsCommands {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn execute(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement TTS commands")
    }
}
