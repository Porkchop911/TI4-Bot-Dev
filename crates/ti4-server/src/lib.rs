//! `ti4-server` provides authoritative server logic, wire protocol data transfer objects,
//! session workers, and redacted game projections for online multiplayer Twilight Imperium 4.

#![allow(clippy::missing_panics_doc, clippy::missing_errors_doc)]

pub mod dev;
pub mod feedback;
pub mod fixtures;
pub mod http;
pub mod maps;
pub mod planning;
pub mod public;
pub mod session;
pub mod storage;
pub mod ws;

pub use http::create_app;
pub use session::{
    BotServiceConfig, GameRegistry, GameSession, MockClient, RemoteHumanDecider, SeatController,
    SessionConfig,
};
pub use storage::{FileGameStore, GameInitRecord, StorageError};
pub use ti4_host::projection;
pub use ti4_host::protocol;
pub use ti4_host::protocol::*;
pub use ti4_host::{card_set, map, preset};
