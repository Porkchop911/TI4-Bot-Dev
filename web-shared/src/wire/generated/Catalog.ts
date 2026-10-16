// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ComponentCatalog } from "./ComponentCatalog.ts";
import type { ContentRecords } from "./ContentRecords.ts";
import type { ExplorationCatalog } from "./ExplorationCatalog.ts";

/**
 * Everything printed that a client is handed once.
 */
export type Catalog = { 
/**
 * The digest of the corpus that the texts are of ([`ti4_content::provenance::digest_of`]).
 * A page compares it with `content` of the snapshot of a server: with another corpus the
 * ids of a game could name other cards.
 */
content: string, exploration: ExplorationCatalog, components: ComponentCatalog, 
/**
 * Every record of the corpus that a page names by id, of every source: the exploration and
 * the components above are those of `sources` only.
 */
records: ContentRecords, };
