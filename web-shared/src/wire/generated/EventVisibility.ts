// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * Explicit audience for an authoritative event.
 */
export type EventVisibility = { "visibility": "public" } | { "visibility": "seat", "seat": PlayerId } | { "visibility": "referee" };
