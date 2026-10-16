//! The hosted game, natively: one test crate, so the `slow_` tier is found by module path
//! (`cargo t-web-slow`). The exports keep one game in statics: every test takes `common::SERIAL`.

mod common;
mod feedback_offline;
mod host_batch;
mod host_history;
mod host_session;
mod host_setup;
mod server_parity;
