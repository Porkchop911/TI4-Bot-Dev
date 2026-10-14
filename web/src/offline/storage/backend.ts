/**
 * Low-level storage: one interface, an IndexedDB implementation (versioned schema + migrations)
 * and an in-memory one (the fallback when IndexedDB is unavailable, and for tests).
 *
 * Values are plain JSON-able objects (structured-cloneable). Nothing here validates content;
 * `saveStore.ts` does that on every read.
 */
import type { StoredFeedbackItem } from "./types.ts";

export interface Backend {
  readonly kind: "indexeddb" | "memory";
  getAllGames(): Promise<unknown[]>;
  putGame(record: { id: string }): Promise<void>;
  deleteGame(id: string): Promise<void>;
  getAllFeedback(): Promise<unknown[]>;
  putFeedback(item: StoredFeedbackItem): Promise<void>;
  deleteFeedback(localId: string): Promise<void>;
  getMeta(key: string): Promise<unknown>;
  setMeta(key: string, value: unknown): Promise<void>;
  close(): void;
}

export const DB_NAME = "ti4-offline";
/** Bump together with a new entry in MIGRATIONS. */
export const DB_VERSION = 1;

/**
 * `MIGRATIONS[n]` upgrades a database from version n to n + 1. Run in order inside the
 * versionchange transaction. Never edit an old step; add a new one.
 */
export const MIGRATIONS: Array<(db: IDBDatabase, tx: IDBTransaction) => void> = [
  // 0 -> 1: the first schema.
  (db) => {
    const games = db.createObjectStore("games", { keyPath: "id" });
    games.createIndex("updatedAt", "updatedAt");
    db.createObjectStore("feedback", { keyPath: "localId" });
    db.createObjectStore("meta");
  },
];

export class BackendOpenError extends Error {
  constructor(
    readonly kind: "unavailable" | "newer-version",
    message: string,
  ) {
    super(message);
    this.name = "BackendOpenError";
  }
}

const req = <T>(r: IDBRequest<T>): Promise<T> =>
  new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error("IndexedDB request failed"));
  });

/** Resolves when the transaction COMMITTED (not when the request succeeded). */
const committed = (tx: IDBTransaction): Promise<void> =>
  new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error("IndexedDB transaction failed"));
    tx.onabort = () => reject(tx.error ?? new DOMException("Transaction aborted", "AbortError"));
  });

export class IdbBackend implements Backend {
  readonly kind = "indexeddb" as const;
  private constructor(private db: IDBDatabase) {
    // Another tab upgrades the schema: step aside.
    db.onversionchange = () => db.close();
  }

  static async open(factory: IDBFactory | undefined, name = DB_NAME): Promise<IdbBackend> {
    if (!factory) throw new BackendOpenError("unavailable", "IndexedDB is not available in this browser.");
    try {
      const open = factory.open(name, DB_VERSION);
      open.onupgradeneeded = (ev) => {
        const tx = open.transaction!;
        for (let v = ev.oldVersion; v < DB_VERSION; v++) MIGRATIONS[v]!(open.result, tx);
      };
      open.onblocked = () => undefined; // waits for the other tab to close; the open then continues
      return new IdbBackend(await req(open));
    } catch (e) {
      const err = e as { name?: string };
      if (err?.name === "VersionError") {
        throw new BackendOpenError("newer-version", "These saved games were written by a newer version of the app.");
      }
      throw new BackendOpenError("unavailable", `Browser storage is not available (${err?.name ?? "error"}).`);
    }
  }

  private async write(store: string, fn: (s: IDBObjectStore) => void): Promise<void> {
    const tx = this.db.transaction(store, "readwrite", { durability: "strict" });
    const done = committed(tx);
    fn(tx.objectStore(store));
    await done;
  }

  async getAllGames() {
    return req(this.db.transaction("games").objectStore("games").getAll());
  }
  putGame(record: { id: string }) {
    return this.write("games", (s) => void s.put(record));
  }
  deleteGame(id: string) {
    return this.write("games", (s) => void s.delete(id));
  }
  getAllFeedback() {
    return req(this.db.transaction("feedback").objectStore("feedback").getAll());
  }
  putFeedback(item: StoredFeedbackItem) {
    return this.write("feedback", (s) => void s.put(item));
  }
  deleteFeedback(localId: string) {
    return this.write("feedback", (s) => void s.delete(localId));
  }
  getMeta(key: string) {
    return req(this.db.transaction("meta").objectStore("meta").get(key));
  }
  setMeta(key: string, value: unknown) {
    return this.write("meta", (s) => void s.put(value, key));
  }
  close() {
    this.db.close();
  }
}

export class MemoryBackend implements Backend {
  readonly kind = "memory" as const;
  private games = new Map<string, unknown>();
  private feedback = new Map<string, unknown>();
  private meta = new Map<string, unknown>();
  async getAllGames() {
    return [...this.games.values()].map((v) => structuredClone(v));
  }
  async putGame(r: { id: string }) {
    this.games.set(r.id, structuredClone(r));
  }
  async deleteGame(id: string) {
    this.games.delete(id);
  }
  async getAllFeedback() {
    return [...this.feedback.values()].map((v) => structuredClone(v));
  }
  async putFeedback(i: StoredFeedbackItem) {
    this.feedback.set(i.localId, structuredClone(i));
  }
  async deleteFeedback(id: string) {
    this.feedback.delete(id);
  }
  async getMeta(k: string) {
    return this.meta.get(k);
  }
  async setMeta(k: string, v: unknown) {
    this.meta.set(k, v);
  }
  close() {}
}
