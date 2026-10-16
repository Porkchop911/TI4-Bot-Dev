/**
 * The seam between the offline UI shell (this folder) and whatever actually runs a local game.
 *
 * T2 (this shell) is built only against these types. T1 introduces the real `GameTransport`
 * interface and T3..T5 provide the wasm host; T5 implements `OfflineLauncher` on top of them and
 * registers it with `registerOfflineLauncher` (see ./README.md). Nothing here imports
 * GameSessionClient or useGameSession.
 */

/** Features that exist only against a server. A transport reports them; the UI hides what is off. */
export interface Capabilities {
  /** The map splice preview in the lobby / map picker. */
  splicePreview: boolean;
  /** Turn redo (undo to the start of the own turn and play it again). */
  turnRedo: boolean;
  /** More than one human: lobby, invite links, presence, takeover. */
  multiplayer: boolean;
  /** Feedback reaches the server at once (offline it is queued and sent later). */
  immediateFeedback: boolean;
}

export const PLAYER_COUNTS = [3, 4, 5, 6] as const;
export type OfflinePlayerCount = (typeof PLAYER_COUNTS)[number];

/** What the setup form produces. One human, every other seat a rule bot. */
export interface OfflineSetup {
  playerCount: OfflinePlayerCount;
  /** 1-based seat of the human; `null` = the host draws one at random. */
  humanSeat: number | null;
  /** Faction id of the human; `null` = random. Bots always draw randomly. */
  humanFaction: string | null;
  nickname: string;
  /** Only ever set from the dev-only seed field. */
  seed?: number;
}

/** One row of "My offline games". */
export interface OfflineGameSummary {
  gameId: string;
  playerCount: number;
  humanFaction: string | null;
  /** The first of `humanSeats`. */
  humanSeat: number;
  /** The seats that people play, 1-based. */
  humanSeats: number[];
  /** The faction of each seat (seat 1 first), null while it is not known. */
  factions: Array<string | null>;
  /**
   * False when this app cannot show the game: several people play it at one screen (a game
   * started in the other app). It can still be exported and deleted.
   */
  playable: boolean;
  round: number | null;
  phase: string | null;
  /** Number of the human's answers stored (the replay length). */
  answers: number;
  /** Unix seconds. */
  createdAt: number;
  /** Unix seconds. */
  updatedAt: number;
  status: "in_progress" | "finished";
  /** Rough resume time in ms from the last measured replay; null when unknown. */
  resumeEstimateMs: number | null;
}

/**
 * Progress of loading the engine and replaying a saved game. `total` is null while it is unknown
 * (the bar is then indeterminate). Replay can take up to about 14 s for a full game.
 */
export interface OfflineProgress {
  phase: "loading-engine" | "replaying" | "starting";
  done: number;
  total: number | null;
}

export type OfflineProgressListener = (progress: OfflineProgress) => void;

/** What a running offline game gives the shell. T5 puts the real `GameTransport` in `transport`. */
export interface OfflineGameHandle {
  gameId: string;
  /** Opaque to the shell. */
  transport: unknown;
  /** The game was opened to look at only (saved with another engine): moves are refused. */
  viewOnly?: boolean;
}

/** Which engine runs (or ran) a game. `wasmSha256` is the sha-256 of the engine file itself. */
export interface OfflineEngineIdentity {
  build: string;
  contentDigest: string;
  wasmSha256: string;
}

/** A saved game that does not fit the engine of this page, and the three things a person may do. */
export interface OfflineMismatchPrompt {
  reason: "engine-changed" | "replay-diverged";
  differs: Array<"build" | "contentDigest" | "wasmSha256">;
  saved: OfflineEngineIdentity;
  current: OfflineEngineIdentity;
  title: string;
  message: string;
  choices: ReadonlyArray<{ id: OfflineMismatchChoice; label: string; destructive: boolean }>;
}
export type OfflineMismatchChoice = "view-only" | "try-anyway" | "delete";

/** Extra arguments of `resume`. */
export interface OfflineResumeOptions {
  /**
   * Asked when the saved game was made with another engine, before the replay, or when the replay
   * gave other decisions than the saved game had. Without it a mismatch refuses to open.
   */
  decide?: (prompt: OfflineMismatchPrompt) => Promise<OfflineMismatchChoice>;
}

/** The resume was answered with "delete this game". */
export class OfflineGameDeletedError extends Error {
  constructor() {
    super("The game was deleted.");
    this.name = "OfflineGameDeletedError";
  }
}

/** State of the browser storage under the saves (T6), for the warning banner and the import button. */
export interface OfflineStorage {
  status(): {
    persistence: "indexeddb" | "memory";
    /** Text of a visible notice; null when all is well. */
    warning: string | null;
    persisted: boolean | null;
    /** Saved games of an older version: not read, not listed, only deleted. */
    unreadable: number;
  };
  subscribe(listener: () => void): () => void;
  /** Deletes the saved games of an older version; resolves with how many. */
  removeUnreadable(): Promise<number>;
  /** Reads a game file exported earlier into the list. Throws with a message fit to show. */
  importGame(file: Blob): Promise<{ id: string; engineDiffers: boolean }>;
}

export type FeedbackCategory = "bug" | "confusing" | "idea" | "other";

/** Everything the server needs to write a normal feedback file, plus what replays an offline game. */
export interface QueuedFeedbackReport {
  category: FeedbackCategory;
  text: string;
  /** The ordinary context of the feedback sheet. */
  context: Record<string, unknown>;
  /** Present for offline games: lets triage replay the game. Sent as `context.local`. */
  local?: OfflineFeedbackLocal;
  /**
   * Client-generated id of this report (8 to 64 characters of A-Za-z0-9_-). The queue keeps it
   * across retries and sends it as `report_id`, so the server stores the report once.
   */
  reportId?: string;
}

/**
 * What `POST /api/feedback` accepts as `context.local` (crates/ti4-server/src/feedback.rs,
 * `validate_local`). `save` is the host's `save_state` object; `seed` and `answers` repeat its
 * values and must equal them. Caps: `context.local` 2 MiB, `answers` 50 000 entries.
 */
export interface OfflineFeedbackLocal {
  seed: number;
  /** The human's option ids so far. */
  answers: string[];
  engine: { build: string; content_digest: string };
  wasm_sha256: string;
  save: {
    format: 2;
    seed: number;
    game: Record<string, unknown>;
    decisions: string[];
    answers: string[];
    redo: string[];
    never: string[];
    event_times: string[];
    event_versions: Array<number | null>;
    generation: number;
    version: number;
    decisions_hash: string;
    [other: string]: unknown;
  };
}

export type QueuedFeedbackState = "queued" | "sending" | "sent" | "failed";

export interface QueuedFeedbackItem {
  localId: string;
  report: QueuedFeedbackReport;
  state: QueuedFeedbackState;
  attempts: number;
  queuedAt: number;
  /** The server's report id once sent. */
  serverId?: string;
}

/**
 * Store-and-forward for offline feedback. Persistent (IndexedDB) in the real launcher. Sending
 * happens in the background and never blocks the game; `retry` forces an attempt now.
 */
export interface OfflineFeedbackQueue {
  enqueue(report: QueuedFeedbackReport): Promise<QueuedFeedbackItem>;
  list(): QueuedFeedbackItem[];
  subscribe(listener: () => void): () => void;
  retry(localId: string): void;
}

export interface OfflineLauncher {
  capabilities: Capabilities;
  /** Faction ids the human can choose, in display order. */
  factions: readonly string[];
  /** False when this browser cannot run the engine (no JSPI): the reason is shown instead. */
  supported(): { ok: true } | { ok: false; reason: string };
  list(): Promise<OfflineGameSummary[]>;
  create(setup: OfflineSetup, onProgress: OfflineProgressListener, signal: AbortSignal): Promise<OfflineGameHandle>;
  resume(
    gameId: string,
    onProgress: OfflineProgressListener,
    signal: AbortSignal,
    options?: OfflineResumeOptions,
  ): Promise<OfflineGameHandle>;
  remove(gameId: string): Promise<void>;
  /** A JSON file with seed, answers and engine hash; the user keeps it as a backup. */
  exportGame(gameId: string): Promise<{ filename: string; blob: Blob }>;
  feedback: OfflineFeedbackQueue;
  /** Where the saves live; absent in builds that cannot import. */
  storage?: OfflineStorage;
}
