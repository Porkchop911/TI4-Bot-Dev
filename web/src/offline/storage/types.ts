/**
 * Types of the offline save store. Self-contained: nothing here imports another offline file
 * except the two shapes of ../types.ts that the store has to agree with.
 */
import type { OfflineGameSummary, OfflineFeedbackQueue, QueuedFeedbackItem } from "../types.ts";

/** Hard limits. Everything that comes from outside (a file, a stored record) is checked against them. */
export const LIMITS = {
  /** One game's save, serialised. A 5000-answer game is about 60 KiB; this is generous. */
  maxSaveBytes: 8 * 1024 * 1024,
  /** An imported file (the save plus its envelope). */
  maxImportBytes: 9 * 1024 * 1024,
  maxGames: 100,
  /** All games together. */
  maxTotalBytes: 128 * 1024 * 1024,
  maxAnswers: 20000,
  maxAnswerChars: 200,
  maxNicknameChars: 40,
  maxIdChars: 80,
  /** Feedback: mirrors the server (`MAX_TEXT_CHARS`, `MAX_CONTEXT_BYTES`). */
  maxFeedbackTextChars: 4000,
  maxFeedbackPayloadBytes: 200 * 1024,
  maxFeedbackItems: 50,
} as const;

/** Which engine produced a save. `wasmSha256` is what the page hashed; the other two come from the save. */
export interface EngineIdentity {
  build: string;
  contentDigest: string;
  wasmSha256: string;
}

/** The T4 `save` object (crates/ti4-wasm/README.md, "Save and resume"). Extra fields are kept untouched. */
export interface HostSave {
  format: 1;
  game_id: string;
  seed: number;
  players: number;
  /** 0-based. */
  human_seat: number;
  setup: Record<string, unknown>;
  /** The human's option ids so far. */
  answers: string[];
  finished: boolean;
  round?: number | null;
  phase?: string | null;
  engine?: { build?: string; content_digest?: string };
  [extra: string]: unknown;
}

export const RECORD_SCHEMA = 1;

/** One stored game (object store `games`). */
export interface SavedGameRecord {
  schema: typeof RECORD_SCHEMA;
  id: string;
  nickname: string;
  createdAt: number;
  updatedAt: number;
  status: "in_progress" | "finished";
  /** Setup summary, for the list without opening the save. */
  summary: {
    playerCount: number;
    /** 1-based, like `OfflineGameSummary`. */
    humanSeat: number;
    humanFaction: string | null;
    seed: number;
    round: number | null;
    phase: string | null;
    answers: number;
  };
  engine: EngineIdentity;
  /** Time of the last measured replay, for the progress estimate. */
  resumeEstimateMs: number | null;
  imported: boolean;
  /** Serialised size of `save`. */
  sizeBytes: number;
  save: HostSave;
}

export type SavedGameSummary = Omit<SavedGameRecord, "save">;

/** Why the store is not (fully) persistent. `null` warning = all is well. */
export interface StoreStatus {
  persistence: "indexeddb" | "memory";
  /** Text for a visible notice; null when nothing is wrong. */
  warning: string | null;
  warningKind: null | "unavailable" | "quota" | "write-failed" | "newer-version";
  /** Result of navigator.storage.persist(); null = not asked yet or not supported. */
  persisted: boolean | null;
  /** Games whose stored record failed validation and are hidden from the list. */
  unreadable: number;
  /** Games held only in memory because their last write failed. */
  unsynced: number;
}

export class StoreError extends Error {
  constructor(
    readonly code: "quota" | "too-many-games" | "too-large" | "invalid" | "not-found" | "write-failed" | "closed",
    message: string,
  ) {
    super(message);
    this.name = "StoreError";
  }
}

/** What the creator of a game tells the store besides the save. */
export interface GameMeta {
  nickname: string;
  humanFaction: string | null;
  engine: EngineIdentity;
  /** Only the first write of a game needs it; later writes keep the old value. */
  createdAt?: number;
  resumeEstimateMs?: number | null;
}

export interface ImportOutcome {
  id: string;
  record: SavedGameSummary;
  /** The file was written by another build: resuming will ask the engine-mismatch question. */
  engineDiffers: boolean;
}

export type EngineMismatchChoice = "view-only" | "try-anyway" | "delete";

/** Data for the dialog "this game was saved with another version of the engine". */
export interface EngineMismatchPrompt {
  /** `engine-changed`: known before replaying; `replay-diverged`: the replay gave other decisions (resume_check.ok false). */
  reason: "engine-changed" | "replay-diverged";
  differs: Array<"build" | "contentDigest" | "wasmSha256">;
  saved: EngineIdentity;
  current: EngineIdentity;
  title: string;
  message: string;
  choices: ReadonlyArray<{ id: EngineMismatchChoice; label: string; destructive: boolean }>;
}

/** The persistent part: games, status, import/export. */
export interface OfflineSaveStore {
  status(): StoreStatus;
  subscribe(listener: () => void): () => void;

  /** Newest first. Records that fail validation are left out (counted in `status().unreadable`). */
  list(): Promise<SavedGameSummary[]>;
  /** The list in the shape the T2 shell shows. */
  listSummaries(): Promise<OfflineGameSummary[]>;
  load(id: string): Promise<SavedGameRecord | null>;
  remove(id: string): Promise<void>;

  /**
   * Stores the save durably (resolves when the transaction committed, or when the in-memory
   * fallback holds it). Call it BEFORE the game goes on with the answer (write-ahead). For the
   * debounced path see `createAutosave`.
   */
  put(id: string, save: HostSave, meta: GameMeta): Promise<void>;

  exportGame(id: string): Promise<{ filename: string; blob: Blob }>;
  /** Parses and validates; never trusts the content. An import always gets a fresh id. Throws StoreError("invalid" | "too-large"). */
  importGame(file: Blob | string, currentEngine?: EngineIdentity): Promise<ImportOutcome>;

  /** Asks navigator.storage.persist() once (idempotent). */
  requestPersistence(): Promise<boolean | null>;
  estimate(): Promise<{ usage: number; quota: number } | null>;

  /** The feedback queue's own storage (same database). */
  feedbackBackend: FeedbackBackend;
  close(): void;
}

/** Low level storage of the feedback queue. */
export interface FeedbackBackend {
  getAll(): Promise<StoredFeedbackItem[]>;
  put(item: StoredFeedbackItem): Promise<void>;
  delete(localId: string): Promise<void>;
}

/** A queue item plus the retry state the UI type does not show. */
export interface StoredFeedbackItem extends QueuedFeedbackItem {
  /** Idempotency key: sent with every attempt; the same report is never created twice. */
  reportId: string;
  /** Not before this time (ms since epoch). */
  nextAttemptAt: number;
  lastError?: string;
  lastAttemptAt?: number;
}

export type FeedbackSendResult =
  | { ok: true; serverId: string }
  | { ok: false; retryable: boolean; message: string };

export type FeedbackSender = (payload: FeedbackPayload) => Promise<FeedbackSendResult>;

/** What the sender gets. `reportId` goes with every attempt of the same report. */
export interface FeedbackPayload {
  reportId: string;
  category: QueuedFeedbackItem["report"]["category"];
  text: string;
  context: Record<string, unknown>;
  local?: QueuedFeedbackItem["report"]["local"];
}

export interface DurableFeedbackQueue extends OfflineFeedbackQueue {
  /** Loads the stored items; call once before use. */
  start(): Promise<void>;
  /** Tries every due item now (also called on `online` and by the timer). */
  flush(): Promise<void>;
  dispose(): void;
}
