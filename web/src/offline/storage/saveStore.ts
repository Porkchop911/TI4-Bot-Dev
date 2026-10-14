import type { OfflineGameSummary } from "../types.ts";
import { type Backend, BackendOpenError, IdbBackend, MemoryBackend } from "./backend.ts";
import {
  type EngineIdentity,
  type FeedbackBackend,
  type GameMeta,
  type HostSave,
  type ImportOutcome,
  LIMITS,
  type OfflineSaveStore,
  type SavedGameRecord,
  type SavedGameSummary,
  type StoreStatus,
  StoreError,
  type StoredFeedbackItem,
} from "./types.ts";
import { engineDifferences } from "./engineCheck.ts";
import { buildExport, buildRecord, parseExport, validateRecord, validateSave } from "./validate.ts";

export interface OpenStoreOptions {
  /** Default: `globalThis.indexedDB`. Pass `undefined` explicitly via `backend` to force memory. */
  indexedDB?: IDBFactory;
  /** Use this backend as it is (tests). */
  backend?: Backend;
  dbName?: string;
  now?: () => number;
  newId?: () => string;
}

const UNAVAILABLE_WARNING =
  "Saving to this browser is not available (a private window, or storage is blocked). " +
  "Your game is kept only while this tab stays open; export it to keep a copy.";
const NEWER_WARNING =
  "The saved games on this device were written by a newer version of the app. " +
  "They cannot be opened here, and new games are kept only while this tab stays open.";
const QUOTA_WARNING =
  "This device is out of storage space, so the latest moves are not saved. Free up space or delete old games.";
const WRITE_WARNING = "The game could not be saved to this browser just now. It will try again after your next move.";

const isQuota = (e: unknown) => (e as { name?: string })?.name === "QuotaExceededError";

/**
 * Opens the store. Never throws for a storage problem: it falls back to memory and says so in
 * `status().warning` (the UI shows it as a visible notice).
 */
export async function openOfflineSaveStore(opts: OpenStoreOptions = {}): Promise<OfflineSaveStore> {
  let backend: Backend;
  let openProblem: { kind: "unavailable" | "newer-version"; text: string } | null = null;
  if (opts.backend) {
    backend = opts.backend;
  } else {
    try {
      let factory = opts.indexedDB;
      if (!factory) {
        try {
          factory = globalThis.indexedDB; // the accessor itself can throw
        } catch {
          factory = undefined;
        }
      }
      backend = await IdbBackend.open(factory, opts.dbName);
    } catch (e) {
      const kind = e instanceof BackendOpenError ? e.kind : "unavailable";
      openProblem = { kind, text: kind === "newer-version" ? NEWER_WARNING : UNAVAILABLE_WARNING };
      backend = new MemoryBackend();
    }
  }
  return new SaveStore(backend, openProblem, opts.now ?? Date.now, opts.newId ?? (() => crypto.randomUUID()));
}

class SaveStore implements OfflineSaveStore {
  private listeners = new Set<() => void>();
  /** Records whose last write failed: the truth until a write succeeds. */
  private shadow = new Map<string, SavedGameRecord>();
  private deleted = new Set<string>();
  private index: Map<string, { sizeBytes: number; createdAt: number }> | null = null;
  private unreadable = 0;
  private persisted: boolean | null = null;
  private warning: { kind: NonNullable<StoreStatus["warningKind"]>; text: string } | null;
  /** Writes of one game go one after the other. */
  private chains = new Map<string, Promise<unknown>>();
  private closed = false;

  constructor(
    private backend: Backend,
    openProblem: { kind: "unavailable" | "newer-version"; text: string } | null,
    private now: () => number,
    private newId: () => string,
  ) {
    this.warning = openProblem && { kind: openProblem.kind, text: openProblem.text };
  }

  // --- status ---------------------------------------------------------------------------

  status(): StoreStatus {
    return {
      persistence: this.backend.kind,
      warning: this.warning?.text ?? null,
      warningKind: this.warning?.kind ?? null,
      persisted: this.persisted,
      unreadable: this.unreadable,
      unsynced: this.shadow.size,
    };
  }
  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  }
  private emit() {
    for (const l of [...this.listeners]) l();
  }
  private setWarning(kind: NonNullable<StoreStatus["warningKind"]> | null, text?: string) {
    const prev = this.warning?.kind ?? null;
    // An open-time problem stays; write problems come and go.
    if (this.warning && (this.warning.kind === "unavailable" || this.warning.kind === "newer-version")) return;
    this.warning = kind ? { kind, text: text! } : null;
    if (prev !== kind) this.emit();
  }

  // --- reading --------------------------------------------------------------------------

  private async readAll(): Promise<Map<string, SavedGameRecord>> {
    const out = new Map<string, SavedGameRecord>();
    let unreadable = 0;
    let rows: unknown[] = [];
    try {
      rows = await this.backend.getAllGames();
    } catch {
      // A failing read shows what is still held in memory.
    }
    for (const row of rows) {
      try {
        const rec = validateRecord(row);
        if (!this.deleted.has(rec.id)) out.set(rec.id, rec);
      } catch {
        unreadable++;
      }
    }
    for (const [id, rec] of this.shadow) {
      const stored = out.get(id);
      if (!stored || stored.updatedAt <= rec.updatedAt) out.set(id, rec);
    }
    if (unreadable !== this.unreadable) {
      this.unreadable = unreadable;
      this.emit();
    }
    this.index = new Map([...out].map(([id, r]) => [id, { sizeBytes: r.sizeBytes, createdAt: r.createdAt }]));
    return out;
  }

  async list(): Promise<SavedGameSummary[]> {
    const all = await this.readAll();
    return [...all.values()]
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map(({ save: _save, ...summary }) => summary);
  }

  async listSummaries(): Promise<OfflineGameSummary[]> {
    return (await this.list()).map((r) => ({
      gameId: r.id,
      playerCount: r.summary.playerCount,
      humanFaction: r.summary.humanFaction,
      humanSeat: r.summary.humanSeat,
      round: r.summary.round,
      phase: r.summary.phase,
      answers: r.summary.answers,
      createdAt: r.createdAt,
      updatedAt: r.updatedAt,
      status: r.status,
      resumeEstimateMs: r.resumeEstimateMs,
    }));
  }

  async load(id: string): Promise<SavedGameRecord | null> {
    return (await this.readAll()).get(id) ?? null;
  }

  // --- writing --------------------------------------------------------------------------

  private serial<T>(id: string, fn: () => Promise<T>): Promise<T> {
    const prev = this.chains.get(id) ?? Promise.resolve();
    const next = prev.then(fn, fn);
    this.chains.set(id, next.catch(() => undefined));
    return next;
  }

  put(id: string, save: HostSave, meta: GameMeta): Promise<void> {
    return this.putRecord(id, save, meta, false);
  }

  private async putRecord(id: string, save: HostSave, meta: GameMeta, imported: boolean): Promise<void> {
    if (this.closed) throw new StoreError("closed", "The store is closed.");
    validateSave(save); // our own caller, but a bad save must never overwrite a good one
    return this.serial(id, async () => {
      if (!this.index) await this.readAll();
      const known = this.index!.get(id);
      const previous = this.shadow.get(id);
      if (!known) {
        if (this.index!.size >= LIMITS.maxGames) {
          throw new StoreError("too-many-games", `Up to ${LIMITS.maxGames} games can be kept. Delete some first.`);
        }
      }
      const total = [...this.index!].reduce((n, [k, v]) => n + (k === id ? 0 : v.sizeBytes), 0);
      const rec = buildRecord(
        id,
        save,
        {
          nickname: meta.nickname,
          humanFaction: meta.humanFaction,
          engine: meta.engine,
          createdAt: known?.createdAt ?? previous?.createdAt ?? meta.createdAt ?? this.now(),
          resumeEstimateMs: meta.resumeEstimateMs ?? previous?.resumeEstimateMs ?? null,
          imported: imported || (previous?.imported ?? false),
        },
        this.now(),
      );
      if (rec.sizeBytes > LIMITS.maxSaveBytes) throw new StoreError("too-large", "The saved game is too large.");
      if (total + rec.sizeBytes > LIMITS.maxTotalBytes) {
        throw new StoreError("quota", "The saved games take too much space. Delete some first.");
      }
      await this.writeRecord(rec);
    });
  }

  /** Writes; on failure keeps the record in memory and raises the warning instead of throwing. */
  private async writeRecord(rec: SavedGameRecord): Promise<void> {
    this.deleted.delete(rec.id);
    try {
      await this.backend.putGame(rec);
      this.index?.set(rec.id, { sizeBytes: rec.sizeBytes, createdAt: rec.createdAt });
      if (this.shadow.delete(rec.id) || this.warning?.kind === "quota" || this.warning?.kind === "write-failed") {
        if (this.shadow.size === 0) this.setWarning(null);
        this.emit();
      }
    } catch (e) {
      this.shadow.set(rec.id, rec);
      this.index?.set(rec.id, { sizeBytes: rec.sizeBytes, createdAt: rec.createdAt });
      if (isQuota(e)) this.setWarning("quota", QUOTA_WARNING);
      else this.setWarning("write-failed", WRITE_WARNING);
      this.emit();
    }
  }

  async remove(id: string): Promise<void> {
    return this.serial(id, async () => {
      this.shadow.delete(id);
      this.index?.delete(id);
      try {
        await this.backend.deleteGame(id);
        this.deleted.delete(id);
      } catch {
        this.deleted.add(id); // hidden for this session at least
        this.setWarning("write-failed", WRITE_WARNING);
      }
      this.emit();
    });
  }

  // --- export / import ------------------------------------------------------------------

  async exportGame(id: string): Promise<{ filename: string; blob: Blob }> {
    const rec = await this.load(id);
    if (!rec) throw new StoreError("not-found", "This game is not stored on this device.");
    const slug = rec.nickname.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 24);
    const file = buildExport(rec, this.now());
    return {
      filename: `ti4-offline-${slug ? `${slug}-` : ""}${rec.id.slice(0, 8)}.json`,
      blob: new Blob([JSON.stringify(file)], { type: "application/json" }),
    };
  }

  async importGame(file: Blob | string, currentEngine?: EngineIdentity): Promise<ImportOutcome> {
    if (typeof file !== "string" && file.size > LIMITS.maxImportBytes) {
      throw new StoreError("too-large", "The file is too large.");
    }
    const text = typeof file === "string" ? file : await readText(file);
    const game = parseExport(text);
    // Always a fresh id: an import never replaces an existing game.
    const id = this.newId();
    await this.putRecord(
      id,
      game.save,
      { nickname: game.nickname, humanFaction: game.humanFaction, engine: game.engine, createdAt: game.createdAt },
      true,
    );
    const rec = await this.load(id);
    if (!rec) throw new StoreError("write-failed", "The game could not be stored.");
    const { save: _s, ...summary } = rec;
    return {
      id,
      record: summary,
      engineDiffers: currentEngine ? engineDifferences(rec.engine, currentEngine).length > 0 : false,
    };
  }

  // --- persistence ----------------------------------------------------------------------

  async requestPersistence(): Promise<boolean | null> {
    if (this.persisted !== null) return this.persisted;
    try {
      const storage = navigator.storage;
      if (!storage?.persist) return null;
      this.persisted = (await storage.persisted?.()) || (await storage.persist());
      this.emit();
    } catch {
      this.persisted = null;
    }
    return this.persisted;
  }

  async estimate() {
    try {
      const e = await navigator.storage?.estimate?.();
      return e && typeof e.usage === "number" && typeof e.quota === "number" ? { usage: e.usage, quota: e.quota } : null;
    } catch {
      return null;
    }
  }

  // --- feedback storage -----------------------------------------------------------------

  feedbackBackend: FeedbackBackend = {
    getAll: async () => {
      try {
        return (await this.backend.getAllFeedback()) as StoredFeedbackItem[];
      } catch {
        return [];
      }
    },
    put: async (item) => {
      try {
        await this.backend.putFeedback(item);
      } catch (e) {
        // The queue keeps the item in memory; only the warning tells it is not durable.
        this.setWarning(isQuota(e) ? "quota" : "write-failed", isQuota(e) ? QUOTA_WARNING : WRITE_WARNING);
      }
    },
    delete: async (id) => {
      try {
        await this.backend.deleteFeedback(id);
      } catch {
        // A leftover row is harmless: the queue drops sent items again on the next load.
      }
    },
  };

  close() {
    this.closed = true;
    this.backend.close();
  }
}

function readText(blob: Blob): Promise<string> {
  if (typeof blob.text === "function") return blob.text();
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error);
    r.readAsText(blob);
  });
}
