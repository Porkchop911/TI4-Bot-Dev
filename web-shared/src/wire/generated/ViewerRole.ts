// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * Viewer role determining redaction boundary.
 */
export type ViewerRole = { "role": "player", "seat": PlayerId } | { "role": "spectator" };
