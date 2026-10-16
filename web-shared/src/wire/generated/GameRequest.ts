// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Local } from "./Local.ts";
import type { Setup } from "./Setup.ts";
import type { TableRequest } from "./TableRequest.ts";

/**
 * The game of a request or of a save: one of `setup`, `table` and `local`.
 */
export type GameRequest = { setup?: Setup, table?: TableRequest, local?: Local, };
