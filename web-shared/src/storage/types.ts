/** Types of the save store of single player games. Self-contained: nothing here imports another file. */

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
  /** The decisions of every seat: about nine for each answer of the person in a game of eight. */
  maxDecisions: 200000,
  maxAnswerChars: 200,
  maxNicknameChars: 40,
  maxIdChars: 80,
  maxSlotChars: 120,
  maxFactionChars: 80,
} as const;

/** Which engine produced a save. `wasmSha256` is what the page hashed; the other two come from the save. */
export interface EngineIdentity {
  build: string;
  contentDigest: string;
  wasmSha256: string;
}

/** The engine's `save` object, format 2 (crates/ti4-wasm/README.md, "Save and resume"). Extra fields are kept untouched. */
export interface HostSave {
  format: 2;
  seed: number;
  /** 2 to 8. */
  players: number;
  /** The game that the save plays: `{setup}`, `{table}` or `{local}`. */
  game: { setup?: Record<string, unknown>; table?: unknown; local?: unknown };
  /** The seats that the person plays at the start, as a mask (bit 0 is seat `a`). */
  humans: number;
  /** Every decision of the game, of every seat, in order. */
  decisions: string[];
  /** The human's option ids so far (among `decisions`). */
  answers: string[];
  finished: boolean;
  round?: number | null;
  phase?: string | null;
  engine?: { build?: string; content_digest?: string };
  [extra: string]: unknown;
}

export const RECORD_SCHEMA = 2;

/** One stored game (object store `games`). */
export interface SavedGameRecord {
  schema: typeof RECORD_SCHEMA;
  id: string;
  /**
   * The fixed address of the game in an app: `"single"`, `"local.<seed>.<players>.<mask>"`,
   * `"scenario.<id>.<mask>"`. Absent for a game that the app finds by its id only.
   */
  slot?: string;
  nickname: string;
  createdAt: number;
  updatedAt: number;
  status: "in_progress" | "finished";
  /** Setup summary, for the list without opening the save. */
  summary: {
    playerCount: number;
    /** The seats that the person plays at the start, as a mask (bit 0 is seat `a`). */
    humans: number;
    /** The faction of each seat, null while it is not known; `playerCount` entries. */
    factions: Array<string | null>;
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

/** Why the store is not (fully) persistent. A null `warningKind` means all is well. */
export interface StoreStatus {
  persistence: "indexeddb" | "memory";
  /** Why a visible notice is due (the app has the text); null when nothing is wrong. */
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
    readonly code:
      | "quota"
      | "too-many-games"
      | "too-large"
      | "invalid"
      | "not-found"
      | "write-failed"
      | "closed",
    message: string,
  ) {
    super(message);
    this.name = "StoreError";
  }
}

/** What the creator of a game tells the store besides the save. */
export interface GameMeta {
  nickname: string;
  /** The faction of each seat, null while it is not known. */
  factions: Array<string | null>;
  engine: EngineIdentity;
  /** The game's slot (`SavedGameRecord.slot`); a later write without it keeps the old one. */
  slot?: string;
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

/** The persistent part: games, status, import/export. */
export interface OfflineSaveStore {
  status(): StoreStatus;
  subscribe(listener: () => void): () => void;

  /** Newest first. Records that fail validation are left out (counted in `status().unreadable`). */
  list(): Promise<SavedGameSummary[]>;
  load(id: string): Promise<SavedGameRecord | null>;
  /** The newest game with this slot, or null. */
  findSlot(slot: string): Promise<SavedGameRecord | null>;
  remove(id: string): Promise<void>;
  /**
   * Deletes the records that are not read (`status().unreadable`: an older save format or record
   * schema, a broken row). Resolves with how many were deleted.
   */
  removeUnreadable(): Promise<number>;

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

  /** The storage of an app's feedback queue (same database, object store `feedback`). */
  feedbackBackend: FeedbackBackend;
  close(): void;
}

/** Low level storage of an app's feedback queue. The rows are the app's; the store only keys them. */
export interface FeedbackBackend {
  /** Unchecked rows: the app validates them. */
  getAll(): Promise<unknown[]>;
  put(item: FeedbackRow): Promise<void>;
  delete(localId: string): Promise<void>;
}

/** A row of the object store `feedback`. */
export interface FeedbackRow {
  localId: string;
}
