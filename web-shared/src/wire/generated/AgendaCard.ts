// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One agenda as printed. Every agenda is in the deck once.
 */
export type AgendaCard = { id: string, name: string, 
/**
 * `Law` or `Directive`.
 */
kind: string, 
/**
 * What the vote is about, as printed: `For/Against`, `Elect Player`, `Elect Planet`. Empty
 * for an agenda that has no outcomes of its own (Covert Legislation).
 */
elect?: string, 
/**
 * The text of the outcome "For", or of the election. A note that the card prints with what
 * is elected is before it, in its brackets.
 */
text: string, 
/**
 * The text of the outcome "Against". Empty for an election.
 */
against?: string, 
/**
 * What the agenda affects, one of [`ComponentCatalog::agenda_groups`].
 */
group: string, };
