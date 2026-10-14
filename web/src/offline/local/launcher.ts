import { OFFLINE_CAPABILITIES } from "../capabilities.ts";
import { setLoadedEngine } from "../engineInfo.ts";
import { OFFLINE_FACTION_IDS } from "../factions.ts";
import {
  engineMismatchPrompt,
  replayDivergedPrompt,
  type EngineIdentity,
  type OfflineSaveStore,
  type SavedGameRecord,
} from "../storage/index.ts";
import {
  OfflineGameDeletedError,
  type OfflineGameHandle,
  type OfflineFeedbackQueue,
  type OfflineLauncher,
  type OfflineMismatchChoice,
  type OfflineProgressListener,
  type OfflineResumeOptions,
  type OfflineSetup,
} from "../types.ts";
import type { HostSave, StartConfig } from "./hostApi.ts";
import { JSPI_MISSING, jspiSupported } from "./jspi.ts";
import type { LocalTransport } from "./LocalTransport.ts";

export { JSPI_MISSING, jspiSupported };

/** Hands out the lazily loaded session code (a dynamic import, so it is a separate chunk). */
const sessionModule = () => import("./session.ts");

/** The 24 faction ids of the engine's `describe`; a test keeps this list in step with the wasm. */
export const OFFLINE_FACTIONS: readonly string[] = OFFLINE_FACTION_IDS;

export interface LocalLauncherOptions {
  /** The save store (IndexedDB, or its memory fallback), opened by the caller. */
  store: OfflineSaveStore;
  /** Shared with the page-level proxy so the banner and the sheet see the same queue. */
  feedback: OfflineFeedbackQueue;
  /** For tests: a different loader of the session code. */
  session?: typeof sessionModule;
}

const nowSeconds = () => Math.floor(Date.now() / 1000);
const abortError = () => new DOMException("aborted", "AbortError");

/** 0-based `Setup.human_seat` from the shell's 1-based seat; the nickname never reaches the engine. */
export function engineSetup(setup: OfflineSetup, gameId: string): Record<string, unknown> {
  const seed = setup.seed ?? randomSeed();
  return {
    game_id: gameId,
    seed,
    players: setup.playerCount,
    ...(setup.humanSeat !== null ? { human_seat: setup.humanSeat - 1 } : {}),
    ...(setup.humanFaction !== null ? { human_faction: setup.humanFaction } : {}),
  };
}

function randomSeed(): number {
  const words = crypto.getRandomValues(new Uint32Array(2));
  // Up to 2^52: the engine takes seeds up to 2^53.
  return (words[0] % 0x100000) * 0x1_0000_0000 + words[1];
}

const newGameId = () => `off_${crypto.randomUUID().replace(/-/g, "").slice(0, 12)}`;

/** The five things a store write needs besides the save. */
interface Entry {
  id: string;
  nickname: string;
  humanFaction: string | null;
  createdAt: number;
  resumeMs: number | null;
  wasmSha256: string;
  /** Looking only: nothing is written. */
  viewOnly: boolean;
}

export function createLocalLauncher(options: LocalLauncherOptions): OfflineLauncher {
  const loadSession = options.session ?? sessionModule;
  const store = options.store;
  const live = new Map<string, LocalTransport>();
  const feedback = options.feedback;
  /** Known after the first engine load; lets an import say whether the file's engine differs. */
  let currentEngine: EngineIdentity | undefined;

  /** Serialised writes per game: a slow store never reorders saves. The store never throws for storage problems. */
  const writes = new Map<string, Promise<void>>();
  const persist = (entry: Entry, save: HostSave): Promise<void> => {
    if (entry.viewOnly) return Promise.resolve();
    const run = async () => {
      await store.put(entry.id, save as never, {
        nickname: entry.nickname,
        humanFaction: (save.setup.human_faction as string | undefined) ?? entry.humanFaction,
        engine: {
          build: save.engine?.build ?? "unknown",
          contentDigest: save.engine?.content_digest ?? "unknown",
          wasmSha256: entry.wasmSha256,
        },
        createdAt: entry.createdAt,
        resumeEstimateMs: entry.resumeMs,
      });
    };
    const chain = (writes.get(entry.id) ?? Promise.resolve()).then(run);
    const settled = chain.catch(() => undefined);
    writes.set(entry.id, settled);
    return settled;
  };

  async function open(
    entry: Entry,
    config: StartConfig,
    onProgress: OfflineProgressListener,
    signal: AbortSignal,
    replaying: boolean,
    engine: Awaited<ReturnType<Awaited<ReturnType<typeof sessionModule>>["loadEngine"]>>,
    session: Awaited<ReturnType<typeof sessionModule>>,
    decide: OfflineResumeOptions["decide"],
    current: EngineIdentity,
    saved: EngineIdentity | null,
  ): Promise<OfflineGameHandle> {
    const total = replaying ? (config.resume?.answers?.length ?? null) : null;
    onProgress({ phase: replaying ? "replaying" : "starting", done: 0, total });
    const started = performance.now();
    const transport = session.openTransport(engine, {
      gameId: entry.id,
      nickname: entry.nickname,
      engineHash: engine.hash,
      wasmSha256: engine.wasmSha256,
      viewOnly: entry.viewOnly,
      config,
      onSave: (save) => {
        entry.humanFaction = transport.humanFaction() ?? entry.humanFaction;
        return persist(entry, save);
      },
      onProgress: (progress) => onProgress({ phase: "replaying", done: progress.done, total: progress.total }),
      onDiverged:
        decide && saved
          ? async (check) => {
              const prompt = replayDivergedPrompt(check, saved, current);
              if (!prompt) return "try-anyway";
              const choice = await decide(prompt);
              // From here on the game is only looked at: no write may replace the saved game.
              if (choice === "view-only") entry.viewOnly = true;
              return choice;
            }
          : undefined,
    });
    const abort = () => transport.close();
    signal.addEventListener("abort", abort, { once: true });
    try {
      await transport.begin();
      await transport.ready;
    } catch (error) {
      transport.close();
      if (transport.deletedOnRequest) {
        await writes.get(entry.id);
        await store.remove(entry.id);
        throw new OfflineGameDeletedError();
      }
      if (signal.aborted) throw abortError();
      throw error;
    } finally {
      signal.removeEventListener("abort", abort);
    }
    if (signal.aborted) {
      transport.close();
      throw abortError();
    }
    if (replaying) entry.resumeMs = Math.round(performance.now() - started);
    live.set(entry.id, transport);
    const save = transport.latestSave();
    if (save && !transport.viewOnly) await persist(entry, save);
    return { gameId: entry.id, transport, ...(transport.viewOnly ? { viewOnly: true } : {}) };
  }

  /** The engine of this page: the file loaded, with the build the engine reports about itself. */
  async function loadCurrent(onProgress: OfflineProgressListener, saved: EngineIdentity | null) {
    onProgress({ phase: "loading-engine", done: 0, total: null });
    const session = await loadSession();
    const engine = await session.loadEngine();
    setLoadedEngine(engine.wasmSha256);
    let current: EngineIdentity;
    try {
      current = session.engineIdentity
        ? await session.engineIdentity(engine)
        : { build: saved?.build ?? "unknown", contentDigest: saved?.contentDigest ?? "unknown", wasmSha256: engine.wasmSha256 };
    } catch {
      // Not knowing the build must not stop a game: the file hash alone still tells.
      current = { build: saved?.build ?? "unknown", contentDigest: saved?.contentDigest ?? "unknown", wasmSha256: engine.wasmSha256 };
    }
    currentEngine = current;
    return { session, engine, current };
  }

  return {
    capabilities: OFFLINE_CAPABILITIES,
    factions: OFFLINE_FACTIONS,
    supported: () => (jspiSupported() ? { ok: true } : { ok: false, reason: JSPI_MISSING }),
    list: () => store.listSummaries(),
    async create(setup, onProgress, signal) {
      if (!jspiSupported()) throw new Error(JSPI_MISSING);
      const gameId = newGameId();
      const config: StartConfig = { setup: engineSetup(setup, gameId) };
      const { session, engine, current } = await loadCurrent(onProgress, null);
      if (signal.aborted) throw abortError();
      const entry: Entry = {
        id: gameId,
        nickname: setup.nickname,
        humanFaction: setup.humanFaction,
        createdAt: nowSeconds(),
        resumeMs: null,
        wasmSha256: engine.wasmSha256,
        viewOnly: false,
      };
      const handle = await open(entry, config, onProgress, signal, false, engine, session, undefined, current, null);
      void store.requestPersistence();
      return handle;
    },
    async resume(gameId, onProgress, signal, resumeOptions) {
      const existing = live.get(gameId);
      if (existing && existing.getState().status === "connected")
        return { gameId, transport: existing, ...(existing.viewOnly ? { viewOnly: true } : {}) };
      existing?.close();
      live.delete(gameId);
      if (!jspiSupported()) throw new Error(JSPI_MISSING);
      const record: SavedGameRecord | null = await store.load(gameId);
      if (!record) throw new Error("This offline game is not on this device any more.");
      const { session, engine, current } = await loadCurrent(onProgress, record.engine);
      if (signal.aborted) throw abortError();
      const entry: Entry = {
        id: gameId,
        nickname: record.nickname,
        humanFaction: record.summary.humanFaction,
        createdAt: record.createdAt,
        resumeMs: record.resumeEstimateMs,
        wasmSha256: engine.wasmSha256,
        viewOnly: false,
      };
      const prompt = engineMismatchPrompt(record.engine, current);
      if (prompt) {
        // Nothing is replayed, and nothing in the store changes, before the person has chosen.
        if (!resumeOptions?.decide) throw new Error(`${prompt.title}. Open the game from the list to choose what to do.`);
        const choice: OfflineMismatchChoice = await resumeOptions.decide(prompt);
        if (signal.aborted) throw abortError();
        if (choice === "delete") {
          await store.remove(gameId);
          throw new OfflineGameDeletedError();
        }
        entry.viewOnly = choice === "view-only";
      }
      const save = record.save as unknown as HostSave;
      const config: StartConfig = { setup: save.setup, resume: save };
      void store.requestPersistence();
      return open(entry, config, onProgress, signal, save.answers.length > 0, engine, session, resumeOptions?.decide, current, record.engine);
    },
    async remove(gameId) {
      live.get(gameId)?.close();
      live.delete(gameId);
      await writes.get(gameId);
      await store.remove(gameId);
    },
    async exportGame(gameId) {
      return store.exportGame(gameId);
    },
    feedback,
    storage: {
      status: () => store.status(),
      subscribe: (listener) => store.subscribe(listener),
      importGame: async (file) => {
        const outcome = await store.importGame(file, currentEngine);
        return { id: outcome.id, engineDiffers: outcome.engineDiffers };
      },
    },
  };
}
