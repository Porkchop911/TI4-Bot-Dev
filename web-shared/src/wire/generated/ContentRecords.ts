// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { AbilityRecord } from "./AbilityRecord.ts";
import type { ActionCardRecord } from "./ActionCardRecord.ts";
import type { AttachmentRecord } from "./AttachmentRecord.ts";
import type { BreakthroughRecord } from "./BreakthroughRecord.ts";
import type { ExploreRecord } from "./ExploreRecord.ts";
import type { FactionRecord } from "./FactionRecord.ts";
import type { LeaderRecord } from "./LeaderRecord.ts";
import type { ObjectiveRecord } from "./ObjectiveRecord.ts";
import type { PlanetRecord } from "./PlanetRecord.ts";
import type { PromissoryNoteRecord } from "./PromissoryNoteRecord.ts";
import type { RelicRecord } from "./RelicRecord.ts";
import type { StrategyCardRecord } from "./StrategyCardRecord.ts";
import type { TechnologyRecord } from "./TechnologyRecord.ts";
import type { UnitRecord } from "./UnitRecord.ts";

/**
 * The records of the corpus that a page names by id, of every source. Each kind is in the
 * order of the corpus, and each record has its id.
 */
export type ContentRecords = { strategy_cards: Array<StrategyCardRecord>, secret_objectives: Array<ObjectiveRecord>, public_objectives: Array<ObjectiveRecord>, action_cards: Array<ActionCardRecord>, technologies: Array<TechnologyRecord>, explores: Array<ExploreRecord>, planets: Array<PlanetRecord>, attachments: Array<AttachmentRecord>, factions: Array<FactionRecord>, units: Array<UnitRecord>, leaders: Array<LeaderRecord>, abilities: Array<AbilityRecord>, promissory_notes: Array<PromissoryNoteRecord>, relics: Array<RelicRecord>, breakthroughs: Array<BreakthroughRecord>, };
