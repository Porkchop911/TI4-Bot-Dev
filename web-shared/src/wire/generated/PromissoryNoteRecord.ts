// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One promissory note as printed. A note that every faction has is `<color>_<alias>`, and its
 * text names the issuer `<color>`.
 */
export type PromissoryNoteRecord = { id: string, name: string, 
/**
 * The faction of a note of a faction. Empty for a note that every faction has.
 */
faction?: string, text?: string, };
