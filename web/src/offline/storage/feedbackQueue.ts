/**
 * Durable store-and-forward queue for feedback written during offline games. Implements
 * `OfflineFeedbackQueue` of ../types.ts (what T2's `useFeedbackQueue` reads) on top of the
 * shared save store's `feedbackBackend` (object store `feedback` of `ti4-offline`).
 *
 * - Durable: every state change is written before the next step; a crash while "sending" reverts
 *   to "queued" on the next start.
 * - Idempotent: each report has a `reportId` (the one the caller gave, else made at enqueue) sent
 *   as `report_id` with every attempt, so the server stores a re-sent report once (it answers
 *   `duplicate: true`, which counts as sent). `enqueue` with a known `reportId` returns the
 *   existing item.
 * - Retry with exponential backoff and jitter; a non-retryable answer (4xx other than 408/429)
 *   or too many attempts ends in "failed" until `retry()`.
 * - Never blocks the game: `enqueue` only touches local storage; sending is in the background.
 */
import { feedbackRequestBody, newReportId } from "../feedbackRoute.ts";
import type { OfflineFeedbackQueue, QueuedFeedbackItem, QueuedFeedbackReport } from "../types.ts";
import { type FeedbackBackend, StoreError } from "@ti4/shared/storage/index.ts";

/** Mirrors the server (`MAX_TEXT_CHARS`, `MAX_CONTEXT_BYTES`), and how many reports may wait. */
export const FEEDBACK_LIMITS = {
  maxFeedbackTextChars: 4000,
  maxFeedbackPayloadBytes: 200 * 1024,
  maxFeedbackItems: 50,
} as const;

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

export interface FeedbackQueueOptions {
  backend: FeedbackBackend;
  send: FeedbackSender;
  now?: () => number;
  random?: () => number;
  newId?: () => string;
  /** The id a report gets when the caller gave none (the server's `report_id`: 8 to 64 of A-Za-z0-9_-). */
  newReportId?: () => string;
  /** First retry delay; doubles each attempt. Default 30 s. */
  baseDelayMs?: number;
  maxDelayMs?: number;
  /** After this many failed attempts the item becomes "failed" (manual retry). Default 12. */
  maxAttempts?: number;
  /** Sent items are forgotten after this long. Default 24 h. */
  keepSentMs?: number;
  /** Where `online` is listened for; default window when present. */
  onlineTarget?: Pick<EventTarget, "addEventListener" | "removeEventListener"> | null;
}

export function backoffDelay(attempts: number, base: number, max: number, random: number): number {
  const exp = Math.min(max, base * 2 ** Math.max(0, attempts - 1));
  return Math.round(exp * (0.75 + 0.5 * random));
}

export function createDurableFeedbackQueue(opts: FeedbackQueueOptions): DurableFeedbackQueue {
  const now = opts.now ?? Date.now;
  const random = opts.random ?? Math.random;
  const newId = opts.newId ?? (() => crypto.randomUUID());
  const newReport = opts.newReportId ?? newReportId;
  const base = opts.baseDelayMs ?? 30_000;
  const max = opts.maxDelayMs ?? 3_600_000;
  const maxAttempts = opts.maxAttempts ?? 12;
  const keepSent = opts.keepSentMs ?? 24 * 3_600_000;

  let items: StoredFeedbackItem[] = [];
  let snapshot: QueuedFeedbackItem[] = [];
  const listeners = new Set<() => void>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let flushing: Promise<void> | null = null;
  let disposed = false;

  const publish = () => {
    // Replace, never mutate: useSyncExternalStore compares by identity.
    snapshot = items.map(toPublic);
    for (const l of [...listeners]) l();
  };
  const replace = (item: StoredFeedbackItem) => {
    items = items.map((i) => (i.localId === item.localId ? item : i));
  };
  const save = async (item: StoredFeedbackItem) => {
    replace(item);
    publish();
    await opts.backend.put(item);
  };

  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = null;
    if (disposed) return;
    const due = items.filter((i) => i.state === "queued").map((i) => i.nextAttemptAt);
    if (due.length === 0) return;
    const wait = Math.max(0, Math.min(...due) - now());
    // setTimeout cannot hold more than ~24 days; the cap keeps it far below.
    timer = setTimeout(() => void flush(), Math.min(wait, 2 ** 30));
  };

  const attempt = async (item: StoredFeedbackItem) => {
    const attempts = item.attempts + 1;
    await save({ ...item, state: "sending", attempts, lastAttemptAt: now() });
    let result: FeedbackSendResult;
    try {
      result = await opts.send(toPayload(item));
    } catch (e) {
      result = { ok: false, retryable: true, message: e instanceof Error ? e.message : "send failed" };
    }
    const current = items.find((i) => i.localId === item.localId)!;
    if (result.ok) {
      await save({ ...current, state: "sent", serverId: result.serverId, lastError: undefined });
    } else if (!result.retryable || attempts >= maxAttempts) {
      await save({ ...current, state: "failed", lastError: result.message });
    } else {
      await save({
        ...current,
        state: "queued",
        lastError: result.message,
        nextAttemptAt: now() + backoffDelay(attempts, base, max, random()),
      });
    }
  };

  const flush = (): Promise<void> => {
    if (flushing) return flushing;
    flushing = (async () => {
      await Promise.resolve(); // `flushing` must be assigned before the body can finish
      try {
        // Oldest first, one at a time: a flaky network is not hammered.
        for (const item of [...items].sort((a, b) => a.queuedAt - b.queuedAt)) {
          if (disposed) return;
          const fresh = items.find((i) => i.localId === item.localId);
          if (fresh?.state === "queued" && fresh.nextAttemptAt <= now()) await attempt(fresh);
        }
      } finally {
        flushing = null;
        schedule();
      }
    })();
    return flushing;
  };

  const onOnline = () => {
    // Back online: due now, whatever the backoff said.
    items = items.map((i) => (i.state === "queued" ? { ...i, nextAttemptAt: 0 } : i));
    void flush();
  };
  const onlineTarget =
    opts.onlineTarget === undefined ? (typeof window !== "undefined" ? window : null) : opts.onlineTarget;
  onlineTarget?.addEventListener("online", onOnline);

  return {
    async start() {
      const stored = await opts.backend.getAll();
      const t = now();
      const keep: StoredFeedbackItem[] = [];
      for (const raw of stored) {
        if (!isStoredItem(raw)) continue;
        if (raw.state === "sent" && t - raw.queuedAt > keepSent) {
          await opts.backend.delete(raw.localId);
          continue;
        }
        // A crash during "sending": the report may or may not have arrived; reportId makes a re-send safe.
        keep.push(raw.state === "sending" ? { ...raw, state: "queued", nextAttemptAt: 0 } : raw);
      }
      items = keep;
      publish();
      void flush();
    },

    async enqueue(report: QueuedFeedbackReport) {
      if (report.reportId) {
        const existing = items.find((i) => i.reportId === report.reportId);
        if (existing) return toPublic(existing);
      }
      checkReport(report);
      if (items.filter((i) => i.state !== "sent").length >= FEEDBACK_LIMITS.maxFeedbackItems) {
        throw new StoreError("too-many-games", "Too many feedback reports are waiting to be sent.");
      }
      const item: StoredFeedbackItem = {
        localId: newId(),
        reportId: report.reportId ?? newReport(),
        report,
        state: "queued",
        attempts: 0,
        queuedAt: now(),
        nextAttemptAt: 0,
      };
      items = [...items, item];
      publish();
      await opts.backend.put(item);
      void flush();
      return toPublic(item);
    },

    list: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    retry(localId) {
      const item = items.find((i) => i.localId === localId);
      if (!item || (item.state !== "failed" && item.state !== "queued")) return;
      void save({ ...item, state: "queued", attempts: 0, nextAttemptAt: 0 }).then(flush);
    },
    flush,
    dispose() {
      disposed = true;
      if (timer) clearTimeout(timer);
      onlineTarget?.removeEventListener("online", onOnline);
    },
  } as DurableFeedbackQueue;
}

function toPublic(i: StoredFeedbackItem): QueuedFeedbackItem {
  const { reportId: _r, nextAttemptAt: _n, lastError: _e, lastAttemptAt: _l, ...pub } = i;
  return pub;
}

function toPayload(i: StoredFeedbackItem): FeedbackPayload {
  return { reportId: i.reportId, category: i.report.category, text: i.report.text, context: i.report.context, local: i.report.local };
}

function checkReport(r: QueuedFeedbackReport) {
  const bad = (m: string) => new StoreError("invalid", m);
  if (!["bug", "confusing", "idea", "other"].includes(r.category)) throw bad("category");
  if (typeof r.text !== "string" || r.text.trim() === "") throw bad("The report is empty.");
  if ([...r.text].length > FEEDBACK_LIMITS.maxFeedbackTextChars) throw bad("The report text is too long.");
  const bytes = new TextEncoder().encode(JSON.stringify({ c: r.context, l: r.local })).length;
  if (bytes > FEEDBACK_LIMITS.maxFeedbackPayloadBytes) throw new StoreError("too-large", "The report is too large.");
}

function isStoredItem(v: unknown): v is StoredFeedbackItem {
  if (typeof v !== "object" || v === null) return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.localId === "string" &&
    typeof o.reportId === "string" &&
    typeof o.queuedAt === "number" &&
    typeof o.attempts === "number" &&
    typeof o.nextAttemptAt === "number" &&
    ["queued", "sending", "sent", "failed"].includes(o.state as string) &&
    typeof o.report === "object" &&
    o.report !== null
  );
}

/**
 * Default sender: `POST /api/feedback` with the body `feedbackRequestBody` builds (the server
 * contract of crates/ti4-server/src/feedback.rs): `report_id` at the top level makes a retry
 * store the report once, `context.local` carries what replays the game. Any 200 counts as sent,
 * a `duplicate: true` answer included (the first attempt arrived, only its answer was lost).
 */
export function httpFeedbackSender(fetchImpl: typeof fetch = (...a) => fetch(...a), url = "/api/feedback"): FeedbackSender {
  return async (p) => {
    try {
      const res = await fetchImpl(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(
          feedbackRequestBody({ category: p.category, text: p.text, context: p.context, local: p.local, reportId: p.reportId }),
        ),
      });
      if (res.ok) {
        const data = (await res.json().catch(() => ({}))) as { id?: string };
        return { ok: true, serverId: data.id ?? "" };
      }
      const retryable = res.status === 408 || res.status === 429 || res.status >= 500 || res.status === 401;
      return { ok: false, retryable, message: `HTTP ${res.status}` };
    } catch {
      return { ok: false, retryable: true, message: "offline" };
    }
  };
}
