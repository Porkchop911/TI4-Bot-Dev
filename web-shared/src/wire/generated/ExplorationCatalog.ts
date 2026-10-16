// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { AttachmentCard } from "./AttachmentCard.ts";
import type { ExplorationCard } from "./ExplorationCard.ts";

/**
 * The printed side of the exploration. It does not change during a game.
 */
export type ExplorationCatalog = { cards: Array<ExplorationCard>, 
/**
 * The cards that each deck begins a game with, sorted by id.
 */
decks: { [key in string]: Array<string> }, attachments: Array<AttachmentCard>, };
