// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BotKindOption } from "./BotKindOption.ts";
import type { CardSetOption } from "./CardSetOption.ts";
import type { FactionGroup } from "./FactionGroup.ts";
import type { MapTemplateSummary } from "./MapTemplateSummary.ts";
import type { Scenario } from "./Scenario.ts";

/**
 * What the options of a table are, with the maps of its number of seats.
 */
export type TableOptions = { 
/**
 * The maps that build for the number of seats, the one that such a table starts with
 * flagged. With no number: every map.
 */
maps: Array<MapTemplateSummary>, min_players: number, max_players: number, strategy_card_sets: Array<CardSetOption>, faction_groups: Array<FactionGroup>, bot_kinds: Array<BotKindOption>, 
/**
 * The longest wait of a seat that the host plays, in milliseconds.
 */
max_bot_delay_ms: number, 
/**
 * The wait of a table that set none: a value of the host.
 */
default_bot_delay_ms: number, 
/**
 * The games that a table can start from, as the Dev sheet of a local game lists them. A
 * host that starts none (the public server) empties the list.
 */
scenarios: Array<Scenario>, };
