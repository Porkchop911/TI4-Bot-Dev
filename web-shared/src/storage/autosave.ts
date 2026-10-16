import type { GameMeta, HostSave, OfflineSaveStore } from "./types.ts";

export interface AutosaveOptions {
  /** Quiet time before a write. Default 300 ms. */
  debounceMs?: number;
  /** A continuous stream still writes at least this often. Default 2000 ms. */
  maxWaitMs?: number;
  onError?: (id: string, error: unknown) => void;
  /** Where `pagehide` / `visibilitychange` are listened for; default window/document when present. */
  windowTarget?: Pick<EventTarget, "addEventListener" | "removeEventListener"> | null;
  documentTarget?:
    | (Pick<EventTarget, "addEventListener" | "removeEventListener"> & { visibilityState?: string })
    | null;
}

export interface Autosave {
  /**
   * Call after every accepted answer with the host's current `save`. The newest save per game
   * wins (each holds all answers), so coalescing loses nothing; at most the last answer is
   * unwritten for `debounceMs`, and it is flushed when the page is hidden or closed.
   */
  schedule(id: string, save: HostSave, meta: GameMeta): void;
  /** Writes everything pending; resolves when it is committed. Use before leaving the game. */
  flush(): Promise<void>;
  dispose(): void;
}

export function createAutosave(store: OfflineSaveStore, opts: AutosaveOptions = {}): Autosave {
  const debounceMs = opts.debounceMs ?? 300;
  const maxWaitMs = opts.maxWaitMs ?? 2000;
  const pending = new Map<
    string,
    { save: HostSave; meta: GameMeta; firstAt: number; timer: ReturnType<typeof setTimeout> }
  >();
  const inflight = new Set<Promise<void>>();

  const writeNow = (id: string) => {
    const p = pending.get(id);
    if (!p) {
      return;
    }
    clearTimeout(p.timer);
    pending.delete(id);
    const job: Promise<void> = store
      .put(id, p.save, p.meta)
      .catch((e) => opts.onError?.(id, e))
      .finally(() => inflight.delete(job));
    inflight.add(job);
  };

  const flush = async () => {
    for (const id of [...pending.keys()]) {
      writeNow(id);
    }
    await Promise.all([...inflight]);
  };

  const onHide = () => void flush();
  const onVisibility = () => {
    if (doc?.visibilityState === "hidden") {
      void flush();
    }
  };
  const win =
    opts.windowTarget === undefined
      ? typeof window !== "undefined"
        ? window
        : null
      : opts.windowTarget;
  const doc =
    opts.documentTarget === undefined
      ? typeof document !== "undefined"
        ? document
        : null
      : opts.documentTarget;
  win?.addEventListener("pagehide", onHide);
  doc?.addEventListener("visibilitychange", onVisibility);

  return {
    schedule(id, save, meta) {
      const old = pending.get(id);
      if (old) {
        clearTimeout(old.timer);
      }
      const firstAt = old?.firstAt ?? Date.now();
      const wait = Math.max(0, Math.min(debounceMs, firstAt + maxWaitMs - Date.now()));
      pending.set(id, { save, meta, firstAt, timer: setTimeout(() => writeNow(id), wait) });
    },
    flush,
    dispose() {
      win?.removeEventListener("pagehide", onHide);
      doc?.removeEventListener("visibilitychange", onVisibility);
      void flush();
    },
  };
}
