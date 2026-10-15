// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Who plays the seats of the others: `smart` (the rule bot, also `rule`, its name in the form
 * of `web/`), `random`, or `mlp:<name>` (`ti4_host::bots`).
 */
export type Bots = "smart" | "random" | `mlp:${string}`;
