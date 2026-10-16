// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One kind of seat that the host plays itself.
 */
export type BotKindOption = { 
/**
 * The kind as a table names it: `smart`, `random`, or `mlp:<name>` for a model
 * (`ti4_host::bots`).
 */
id: "smart" | "random" | `mlp:${string}`, 
/**
 * The name of the kind; the label of a model.
 */
name: string, description: string, };
