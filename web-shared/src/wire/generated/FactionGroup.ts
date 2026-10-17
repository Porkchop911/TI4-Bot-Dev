// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { FactionOption } from "./FactionOption.ts";

/**
 * The factions of one product, in the order of [`seating::LOBBY_FACTIONS`].
 */
export type FactionGroup = { name: string, factions: Array<FactionOption>, };
