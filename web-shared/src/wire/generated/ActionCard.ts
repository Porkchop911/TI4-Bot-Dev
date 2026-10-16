// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One action card as printed. Copies of a card have ids of their own and one name.
 */
export type ActionCard = { id: string, name: string, 
/**
 * The phase of the card: `Action`, `Agenda`, `Strategy`, `Status` or `Any`.
 */
phase: string, 
/**
 * When the card is played, as printed: `Action`, or a timing.
 */
window: string, 
/**
 * The group of the timing, one of [`ComponentCatalog::action_card_groups`].
 */
group: string, text: string, };
