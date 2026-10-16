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
} from "./types.ts";
import {
  buildExport,
  buildRecord,
  engineDifferences,
  parseExport,
  validateRecord,
  validateSave,
} from "./validate.ts";

export interface OpenStoreOptions {
  /** Default: `globalThis.indexedDB`. Pass `undefined` explicitly via `backend` to force memory. */
  indexedDB?: IDBFactory;
  /** Use this backend as it is (tests). */
  backend?: Backend;
  dbName?: string;
  now?: () => number;
  newId?: () => string;
}

const isQuota = (e: unknown) => (e as { name?: string })?.name === "QuotaExceededError";

/**
 * Opens the store. Never throws for a storage problem: it falls back to memory and says so in
 * `status().warningKind` (the app shows it as a visible notice).
 */
export async function openOfflineSaveStore(opts: OpenStoreOptions = {}): Promise<OfflineSaveStore> {
  let backend: Backend;
  let openProblem: "unavailable" | "newer-version" | null = null;
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
      openProblem = e instanceof BackendOpenError ? e.kind : "unavailable";
      backend = new MemoryBackend();
    }
  }
  return new SaveStore(
    backend,
    openProblem,
    opts.now ?? Date.now,
    opts.newId ?? (() => crypto.randomUUID()),
  );
}

/** What the store keeps of every game between reads: the limits and the fields a later write keeps. */
interface IndexEntry {
  sizeBytes: number;
  createdAt: number;
  slot?: string;
  imported: boolean;
}

const indexEntry = (r: SavedGameRecord): IndexEntry => ({
  sizeBytes: r.sizeBytes,
  createdAt: r.createdAt,
  ...(r.slot !== undefined ? { slot: r.slot } : {}),
  imported: r.imported,
});

class SaveStore implements OfflineSaveStore {
  private listeners = new Set<() => void>();
  /** Records whose last write failed: the truth until a write succeeds. */
  private shadow = new Map<string, SavedGameRecord>();
  private deleted = new Set<string>();
  private index: Map<string, IndexEntry> | null = null;
  private unreadable = 0;
  private persisted: boolean | null = null;
  private warning: StoreStatus["warningKind"];
  /** Writes of one game go one after the other. */
  private chains = new Map<string, Promise<unknown>>();
  private closed = false;

  constructor(
    private backend: Backend,
    openProblem: "unavailable" | "newer-version" | null,
    private now: () => number,
    private newId: () => string,
  ) {
    this.warning = openProblem;
  }

  // --- status ---------------------------------------------------------------------------

  status(): StoreStatus {
    return {
      persistence: this.backend.kind,
      warningKind: this.warning,
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
    for (const l of [...this.listeners]) {
      l();
    }
  }
  private setWarning(kind: StoreStatus["warningKind"]) {
    const prev = this.warning;
    // An open-time problem stays; write problems come and go.
    if (prev === "unavailable" || prev === "newer-version") {
      return;
    }
    this.warning = kind;
    if (prev !== kind) {
      this.emit();
    }
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
        if (!this.deleted.has(rec.id)) {
          out.set(rec.id, rec);
        }
      } catch {
        unreadable++;
      }
    }
    for (const [id, rec] of this.shadow) {
      const stored = out.get(id);
      if (!stored || stored.updatedAt <= rec.updatedAt) {
        out.set(id, rec);
      }
    }
    if (unreadable !== this.unreadable) {
      this.unreadable = unreadable;
      this.emit();
    }
    this.index = new Map([...out].map(([id, r]) => [id, indexEntry(r)]));
    return out;
  }

  async list(): Promise<SavedGameSummary[]> {
    const all = await this.readAll();
    return [...all.values()]
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map(({ save: _save, ...summary }) => summary);
  }

  async load(id: string): Promise<SavedGameRecord | null> {
    return (await this.readAll()).get(id) ?? null;
  }

  async findSlot(slot: string): Promise<SavedGameRecord | null> {
    let found: SavedGameRecord | null = null;
    for (const rec of (await this.readAll()).values()) {
      if (rec.slot === slot && (!found || rec.updatedAt > found.updatedAt)) {
        found = rec;
      }
    }
    return found;
  }

  // --- writing --------------------------------------------------------------------------

  private serial<T>(id: string, fn: () => Promise<T>): Promise<T> {
    const prev = this.chains.get(id) ?? Promise.resolve();
    const next = prev.then(fn, fn);
    this.chains.set(
      id,
      next.catch(() => undefined),
    );
    return next;
  }

  put(id: string, save: HostSave, meta: GameMeta): Promise<void> {
    return this.putRecord(id, save, meta, false);
  }

  private async putRecord(
    id: string,
    save: HostSave,
    meta: GameMeta,
    imported: boolean,
  ): Promise<void> {
    if (this.closed) {
      throw new StoreError("closed", "The store is closed.");
    }
    validateSave(save); // our own caller, but a bad save must never overwrite a good one
    return this.serial(id, async () => {
      if (!this.index) {
        await this.readAll();
      }
      const known = this.index!.get(id);
      const previous = this.shadow.get(id);
      if (!known) {
        if (this.index!.size >= LIMITS.maxGames) {
          throw new StoreError(
            "too-many-games",
            `Up to ${LIMITS.maxGames} games can be kept. Delete some first.`,
          );
        }
      }
      const total = [...this.index!].reduce((n, [k, v]) => n + (k === id ? 0 : v.sizeBytes), 0);
      const rec = buildRecord(
        id,
        save,
        {
          nickname: meta.nickname,
          factions: meta.factions,
          engine: meta.engine,
          slot: meta.slot ?? previous?.slot ?? known?.slot,
          createdAt: known?.createdAt ?? previous?.createdAt ?? meta.createdAt ?? this.now(),
          resumeEstimateMs: meta.resumeEstimateMs ?? previous?.resumeEstimateMs ?? null,
          imported: imported || (previous?.imported ?? known?.imported ?? false),
        },
        this.now(),
      );
      if (rec.sizeBytes > LIMITS.maxSaveBytes) {
        throw new StoreError("too-large", "The saved game is too large.");
      }
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
      this.index?.set(rec.id, indexEntry(rec));
      if (
        this.shadow.delete(rec.id) ||
        this.warning === "quota" ||
        this.warning === "write-failed"
      ) {
        if (this.shadow.size === 0) {
          this.setWarning(null);
        }
        this.emit();
      }
    } catch (e) {
      this.shadow.set(rec.id, rec);
      this.index?.set(rec.id, indexEntry(rec));
      this.setWarning(isQuota(e) ? "quota" : "write-failed");
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
        this.setWarning("write-failed");
      }
      this.emit();
    });
  }

  async removeUnreadable(): Promise<number> {
    let rows: unknown[];
    try {
      rows = await this.backend.getAllGames();
    } catch {
      return 0;
    }
    let removed = 0;
    for (const row of rows) {
      try {
        validateRecord(row);
      } catch {
        const id = (row as { id?: unknown } | null)?.id;
        if (typeof id === "string") {
          try {
            await this.backend.deleteGame(id);
            removed++;
          } catch {
            this.setWarning("write-failed");
          }
        }
      }
    }
    await this.readAll();
    this.emit();
    return removed;
  }

  // --- export / import ------------------------------------------------------------------

  async exportGame(id: string): Promise<{ filename: string; blob: Blob }> {
    const rec = await this.load(id);
    if (!rec) {
      throw new StoreError("not-found", "This game is not stored on this device.");
    }
    const slug = rec.nickname
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 24);
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
      {
        nickname: game.nickname,
        factions: game.factions,
        engine: game.engine,
        createdAt: game.createdAt,
      },
      true,
    );
    const rec = await this.load(id);
    if (!rec) {
      throw new StoreError("write-failed", "The game could not be stored.");
    }
    const { save: _s, ...summary } = rec;
    return {
      id,
      record: summary,
      engineDiffers: currentEngine
        ? engineDifferences(rec.engine, currentEngine).length > 0
        : false,
    };
  }

  // --- persistence ----------------------------------------------------------------------

  async requestPersistence(): Promise<boolean | null> {
    if (this.persisted !== null) {
      return this.persisted;
    }
    try {
      const storage = navigator.storage;
      if (!storage?.persist) {
        return null;
      }
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
      return e && typeof e.usage === "number" && typeof e.quota === "number"
        ? { usage: e.usage, quota: e.quota }
        : null;
    } catch {
      return null;
    }
  }

  // --- feedback storage -----------------------------------------------------------------

  feedbackBackend: FeedbackBackend = {
    getAll: async () => {
      try {
        return await this.backend.getAllFeedback();
      } catch {
        return [];
      }
    },
    put: async (item) => {
      try {
        await this.backend.putFeedback(item);
      } catch (e) {
        // The queue keeps the item in memory; only the warning tells it is not durable.
        this.setWarning(isQuota(e) ? "quota" : "write-failed");
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
  if (typeof blob.text === "function") {
    return blob.text();
  }
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error);
    r.readAsText(blob);
  });
}
