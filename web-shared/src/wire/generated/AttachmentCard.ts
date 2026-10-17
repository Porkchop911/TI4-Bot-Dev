// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One attachment of a planet.
 */
export type AttachmentCard = { id: string, name: string, resources: number, influence: number, tech_specialties?: Array<string>, 
/**
 * The text of the exploration card that attaches it. Empty for an attachment of another
 * origin.
 */
text?: string, };
