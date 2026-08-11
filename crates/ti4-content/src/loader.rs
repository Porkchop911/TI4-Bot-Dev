//! Content loader stub.

use ti4_model::content_types::ContentType;
use std::collections::HashMap;

pub struct ContentLoader {
    base_path: std::path::PathBuf,
}

impl ContentLoader {
    pub fn new(base_path: std::path::PathBuf) -> Self {
        Self { base_path }
    }

    pub fn load(&self, _content_type: ContentType) -> Result<(), anyhow::Error> {
        todo!("M02: implement content loading")
    }

    pub fn load_all(&self) -> Result<(), anyhow::Error> {
        todo!("M02: implement full content loading")
    }
}
