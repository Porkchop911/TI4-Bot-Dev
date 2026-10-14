import { OFFLINE_CAPABILITIES } from "./capabilities.ts";
import { OfflineGameDeletedError } from "./types.ts";
import type {
  OfflineGameHandle,
  OfflineGameSummary,
  OfflineLauncher,
  OfflineMismatchPrompt,
  OfflineProgressListener,
  OfflineSetup,
  QueuedFeedbackItem,
  QueuedFeedbackReport,
  QueuedFeedbackState,
} from "./types.ts";

import { OFFLINE_FACTION_IDS } from "./factions.ts";

export const MOCK_FACTIONS = OFFLINE_FACTION_IDS;

export interface MockOptions {
  games?: OfflineGameSummary[];
  feedback?: { state: QueuedFeedbackState; text?: string }[];
  /** Replay length of a resume, and how long it takes in total. */
  replaySteps?: number;
  replayMs?: number;
  /** Unknown total (indeterminate bar). */
  unknownTotal?: boolean;
  unsupported?: string;
  failResume?: string;
  /** The progress stays at this fraction forever (for screenshots). */
  freezeAt?: number;
  /** Text of the storage warning banner (private window, quota). */
  storageWarning?: string;
  /** A resume asks this question first (a saved game of another engine). */
  mismatch?: OfflineMismatchPrompt;
}

/**
 * In-memory stand-in for the real launcher: development, tests and the screenshot captures.
 * The real one (T5) replaces it; the shell cannot tell the difference.
 */
export function createMockLauncher(options: MockOptions = {}): OfflineLauncher {
  let games = [...(options.games ?? [])];
  let items: QueuedFeedbackItem[] = (options.feedback ?? []).map((f, i) => ({
    localId: `fb_local_${i}`,
    state: f.state,
    attempts: f.state === "failed" ? 2 : 0,
    queuedAt: 0,
    report: { category: "bug", text: f.text ?? "text", context: {} },
  }));
  const listeners = new Set<() => void>();
  const emit = () => {
    items = [...items];
    listeners.forEach((l) => l());
  };
  const steps = options.replaySteps ?? 240;
  const total = options.replayMs ?? 1500;

  const play = async (done: OfflineProgressListener, signal: AbortSignal, replay: boolean) => {
    done({ phase: "loading-engine", done: 0, total: null });
    await sleep(Math.min(400, total / 4), signal);
    if (!replay) {
      done({ phase: "starting", done: 0, total: null });
      return sleep(300, signal);
    }
    const tick = 10;
    const frames = Math.max(1, Math.round(total / tick));
    for (let i = 0; i <= frames; i++) {
      const fraction = options.freezeAt ?? i / frames;
      done({
        phase: "replaying",
        done: Math.round(fraction * steps),
        total: options.unknownTotal ? null : steps,
      });
      if (options.freezeAt !== undefined) return new Promise<void>(() => undefined);
      await sleep(tick, signal);
    }
  };

  return {
    capabilities: OFFLINE_CAPABILITIES,
    factions: MOCK_FACTIONS,
    supported: () => (options.unsupported ? { ok: false, reason: options.unsupported } : { ok: true }),
    list: async () => [...games],
    async create(setup: OfflineSetup, onProgress, signal): Promise<OfflineGameHandle> {
      await play(onProgress, signal, false);
      const gameId = `off_${games.length + 1}`;
      games = [
        {
          gameId, playerCount: setup.playerCount, humanFaction: setup.humanFaction,
          humanSeat: setup.humanSeat ?? 1, round: 1, phase: "strategy", answers: 0,
          createdAt: Date.now() / 1000, updatedAt: Date.now() / 1000, status: "in_progress",
          resumeEstimateMs: 1000,
        },
        ...games,
      ];
      return { gameId, transport: null };
    },
    async resume(gameId, onProgress, signal, resumeOptions) {
      if (options.mismatch && resumeOptions?.decide) {
        const choice = await resumeOptions.decide(options.mismatch);
        if (choice === "delete") {
          games = games.filter((g) => g.gameId !== gameId);
          throw new OfflineGameDeletedError();
        }
      }
      await play(onProgress, signal, true);
      if (options.failResume) throw new Error(options.failResume);
      return { gameId, transport: null };
    },
    remove: async (gameId) => {
      games = games.filter((g) => g.gameId !== gameId);
    },
    exportGame: async (gameId) => ({
      filename: `ti4-offline-${gameId}.json`,
      blob: new Blob([JSON.stringify({ gameId })], { type: "application/json" }),
    }),
    storage: {
      status: () => ({
        persistence: options.storageWarning ? "memory" : "indexeddb",
        warning: options.storageWarning ?? null,
        persisted: options.storageWarning ? null : true,
      }),
      subscribe: () => () => undefined,
      importGame: async () => {
        games = [
          {
            gameId: `off_imported_${games.length + 1}`, playerCount: 4, humanFaction: "sol", humanSeat: 1,
            round: 2, phase: "action", answers: 60, createdAt: Date.now() / 1000, updatedAt: Date.now() / 1000,
            status: "in_progress", resumeEstimateMs: 700,
          },
          ...games,
        ];
        return { id: games[0].gameId, engineDiffers: Boolean(options.mismatch) };
      },
    },
    feedback: {
      async enqueue(report: QueuedFeedbackReport) {
        const item: QueuedFeedbackItem = {
          localId: `fb_local_${items.length}`, report, state: "queued", attempts: 0, queuedAt: Date.now(),
        };
        items = [...items, item];
        emit();
        return item;
      },
      list: () => items,
      subscribe(listener) {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
      retry(localId) {
        items = items.map((i) => (i.localId === localId ? { ...i, state: "queued" as const } : i));
        emit();
      },
    },
  };
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) return reject(new DOMException("aborted", "AbortError"));
    const timer = setTimeout(resolve, ms);
    signal.addEventListener("abort", () => {
      clearTimeout(timer);
      reject(new DOMException("aborted", "AbortError"));
    });
  });
}
