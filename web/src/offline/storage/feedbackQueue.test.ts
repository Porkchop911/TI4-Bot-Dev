import { describe, expect, it, vi } from "vitest";
import {
  type EngineIdentity,
  MemoryBackend,
  type OfflineSaveStore,
  StoreError,
  openOfflineSaveStore,
} from "@ti4/shared/storage/index.ts";
import { engineMismatchPrompt, replayDivergedPrompt } from "./engineCheck.ts";
import {
  backoffDelay,
  createDurableFeedbackQueue,
  FEEDBACK_LIMITS,
  type FeedbackSendResult,
  httpFeedbackSender,
  type StoredFeedbackItem,
} from "./feedbackQueue.ts";
import type { OfflineFeedbackLocal } from "../types.ts";

const ENGINE: EngineIdentity = { build: "b1", contentDigest: "c1", wasmSha256: "w1" };
let ids = 0;

describe("engine check", () => {
  it("is silent when identical and lists the differences otherwise", () => {
    expect(engineMismatchPrompt(ENGINE, ENGINE)).toBeNull();
    const p = engineMismatchPrompt(ENGINE, { ...ENGINE, build: "b2", wasmSha256: "w2" })!;
    expect(p.reason).toBe("engine-changed");
    expect(p.differs).toEqual(["build", "wasmSha256"]);
    expect(p.choices.map((c) => c.id)).toEqual(["view-only", "try-anyway", "delete"]);
    expect(p.choices.find((c) => c.id === "delete")!.destructive).toBe(true);
  });

  it("resume_check.ok false gives the replay-diverged prompt", () => {
    expect(replayDivergedPrompt({ ok: true }, ENGINE, ENGINE)).toBeNull();
    expect(replayDivergedPrompt(undefined, ENGINE, ENGINE)).toBeNull();
    const p = replayDivergedPrompt({ ok: false }, ENGINE, ENGINE)!;
    expect(p.reason).toBe("replay-diverged");
    expect(p.choices).toHaveLength(3);
  });
});

const LOCAL: OfflineFeedbackLocal = {
  seed: 1,
  answers: ["x"],
  engine: { build: "0.1.0", content_digest: "dig" },
  wasm_sha256: "ab".repeat(32),
  save: {
    format: 2, seed: 1, game: { setup: { players: 4 } }, decisions: ["x"], answers: ["x"], redo: [], never: [],
    event_times: [], event_versions: [], generation: 0, version: 1, decisions_hash: "h",
  },
};

describe("feedback queue", () => {
  const report = { category: "bug" as const, text: "broken", context: { a: 1 }, local: LOCAL };

  async function setup(results: FeedbackSendResult[] | ((n: number) => FeedbackSendResult), store?: OfflineSaveStore) {
    const s = store ?? (await openOfflineSaveStore({ backend: new MemoryBackend() }));
    let t = 10_000;
    let n = 0;
    const sent: unknown[] = [];
    const q = createDurableFeedbackQueue({
      backend: s.feedbackBackend,
      send: async (p) => {
        sent.push(p);
        const r = typeof results === "function" ? results(n) : (results[n] ?? results.at(-1)!);
        n++;
        return r;
      },
      now: () => t,
      random: () => 0.5,
      newId: () => `q${++ids}`,
      newReportId: () => `report_${ids}`,
      baseDelayMs: 1000,
      maxDelayMs: 8000,
      maxAttempts: 4,
      onlineTarget: null,
    });
    return { s, q, sent, advance: (ms: number) => (t += ms), clock: () => t };
  }

  it("sends in the background and marks sent", async () => {
    const { q, sent } = await setup([{ ok: true, serverId: "srv1" }]);
    await q.start();
    const item = await q.enqueue(report);
    expect(item.state).toBe("queued");
    await q.flush();
    expect(sent).toHaveLength(1);
    expect((sent[0] as { reportId: string }).reportId).toBeTruthy();
    expect(q.list()[0]).toMatchObject({ state: "sent", serverId: "srv1", attempts: 1 });
    q.dispose();
  });

  it("retries with backoff, then succeeds", async () => {
    const { q, sent, advance } = await setup([
      { ok: false, retryable: true, message: "offline" },
      { ok: false, retryable: true, message: "offline" },
      { ok: true, serverId: "s" },
    ]);
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "queued", attempts: 1 });
    await q.flush(); // not due yet
    expect(sent).toHaveLength(1);
    advance(1000);
    await q.flush();
    expect(q.list()[0]!.attempts).toBe(2);
    advance(1000); // backoff is now 2000
    await q.flush();
    expect(sent).toHaveLength(2);
    advance(1000);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "sent", attempts: 3 });
    expect(new Set(sent.map((p) => (p as { reportId: string }).reportId)).size).toBe(1); // same id every time
    q.dispose();
  });

  it("a non-retryable answer fails at once; retry() resets it", async () => {
    const { q, sent } = await setup((n) => (n === 0 ? { ok: false, retryable: false, message: "HTTP 400" } : { ok: true, serverId: "s" }));
    await q.start();
    const item = await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "failed" });
    q.retry(item.localId);
    await vi.waitFor(() => expect(q.list()[0]!.state).toBe("sent"));
    expect(sent).toHaveLength(2);
    q.dispose();
  });

  it("gives up after maxAttempts", async () => {
    const { q, advance } = await setup([{ ok: false, retryable: true, message: "x" }]);
    await q.start();
    await q.enqueue(report);
    for (let i = 0; i < 6; i++) {
      await q.flush();
      advance(10_000);
    }
    expect(q.list()[0]).toMatchObject({ state: "failed", attempts: 4 });
    q.dispose();
  });

  it("a sender that throws counts as a retryable failure", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const q = createDurableFeedbackQueue({ backend: s.feedbackBackend, send: async () => { throw new Error("boom"); }, onlineTarget: null });
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "queued", attempts: 1 });
    q.dispose();
  });

  it("is durable: items survive a restart, and a crash while sending is re-sent with the same report id", async () => {
    const backend = new MemoryBackend();
    const s = await openOfflineSaveStore({ backend });
    const hang = setup([{ ok: true, serverId: "s" }], s);
    const first = await hang;
    // Simulate a crash mid-send: a sender that never answers.
    const q1 = createDurableFeedbackQueue({ backend: s.feedbackBackend, send: () => new Promise(() => undefined), onlineTarget: null, newId: () => "k1", newReportId: () => "k1_report" });
    await q1.start();
    await q1.enqueue(report);
    await vi.waitFor(async () => expect(((await s.feedbackBackend.getAll()) as StoredFeedbackItem[])[0]!.state).toBe("sending"));
    q1.dispose();
    first.q.dispose();
    const s2 = await openOfflineSaveStore({ backend });
    const sent: string[] = [];
    const q2 = createDurableFeedbackQueue({
      backend: s2.feedbackBackend,
      send: async (p) => {
        sent.push(p.reportId);
        return { ok: true, serverId: "s" };
      },
      onlineTarget: null,
    });
    await q2.start();
    await q2.flush();
    expect(sent).toEqual(["k1_report"]); // reportId == the one created before the crash
    expect(q2.list()[0]!.state).toBe("sent");
    q2.dispose();
  });

  it("enqueue with a known reportId is idempotent", async () => {
    const { q } = await setup([{ ok: false, retryable: true, message: "x" }]);
    await q.start();
    const a = await q.enqueue({ ...report, reportId: "R_report_1" });
    const b = await q.enqueue({ ...report, reportId: "R_report_1" });
    expect(b.localId).toBe(a.localId);
    expect(q.list()).toHaveLength(1);
    q.dispose();
  });

  it("takes the report id the caller made and keeps it across restarts", async () => {
    const { q, sent } = await setup([{ ok: true, serverId: "s" }]);
    await q.start();
    await q.enqueue({ ...report, reportId: "r_0123456789ab" });
    await q.flush();
    expect((sent[0] as { reportId: string }).reportId).toBe("r_0123456789ab");
    q.dispose();
  });

  it("validates and caps reports", async () => {
    const { q } = await setup([{ ok: true, serverId: "s" }]);
    await q.start();
    await expect(q.enqueue({ ...report, text: "  " })).rejects.toMatchObject({ code: "invalid" });
    await expect(q.enqueue({ ...report, text: "x".repeat(FEEDBACK_LIMITS.maxFeedbackTextChars + 1) })).rejects.toMatchObject({ code: "invalid" });
    await expect(q.enqueue({ ...report, context: { blob: "x".repeat(FEEDBACK_LIMITS.maxFeedbackPayloadBytes) } })).rejects.toMatchObject({ code: "too-large" });
    await expect(q.enqueue({ ...report, category: "nope" as never })).rejects.toMatchObject({ code: "invalid" });
    q.dispose();
  });

  it("limits the number of waiting reports", async () => {
    const { q } = await setup([{ ok: false, retryable: true, message: "x" }]);
    await q.start();
    for (let i = 0; i < FEEDBACK_LIMITS.maxFeedbackItems; i++) await q.enqueue(report);
    await expect(q.enqueue(report)).rejects.toBeInstanceOf(StoreError);
    q.dispose();
  });

  it("notifies subscribers with new snapshots (identity changes)", async () => {
    const { q } = await setup([{ ok: true, serverId: "s" }]);
    await q.start();
    const cb = vi.fn();
    const off = q.subscribe(cb);
    const before = q.list();
    await q.enqueue(report);
    expect(cb).toHaveBeenCalled();
    expect(q.list()).not.toBe(before);
    off();
    q.dispose();
  });

  it("works from memory when the database cannot be written", async () => {
    const backend = new MemoryBackend();
    backend.putFeedback = async () => {
      throw new DOMException("full", "QuotaExceededError");
    };
    const s = await openOfflineSaveStore({ backend });
    const { q } = await setup([{ ok: true, serverId: "s" }], s);
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]!.state).toBe("sent");
    expect(s.status().warningKind).toBe("quota");
    q.dispose();
  });

  it("drops old sent items on start", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const old: StoredFeedbackItem = { localId: "old", reportId: "r", report, state: "sent", attempts: 1, queuedAt: 1, nextAttemptAt: 0 };
    await s.feedbackBackend.put(old);
    const q = createDurableFeedbackQueue({ backend: s.feedbackBackend, send: async () => ({ ok: true, serverId: "" }), now: () => 10 * 24 * 3_600_000, onlineTarget: null });
    await q.start();
    expect(q.list()).toEqual([]);
    expect(await s.feedbackBackend.getAll()).toEqual([]);
    q.dispose();
  });

  it("online event makes queued items due now", async () => {
    const target = new EventTarget();
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    let ok = false;
    let t = 0;
    const q = createDurableFeedbackQueue({
      backend: s.feedbackBackend,
      send: async () => (ok ? { ok: true, serverId: "s" } : { ok: false, retryable: true, message: "offline" }),
      now: () => t,
      onlineTarget: target,
    });
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]!.state).toBe("queued");
    ok = true;
    target.dispatchEvent(new Event("online"));
    await vi.waitFor(() => expect(q.list()[0]!.state).toBe("sent"));
    q.dispose();
  });

  it("backoff grows, is capped, and jitters", () => {
    expect(backoffDelay(1, 1000, 8000, 0.5)).toBe(1000);
    expect(backoffDelay(3, 1000, 8000, 0.5)).toBe(4000);
    expect(backoffDelay(9, 1000, 8000, 0.5)).toBe(8000);
    expect(backoffDelay(1, 1000, 8000, 0)).toBe(750);
    expect(backoffDelay(1, 1000, 8000, 1)).toBe(1250);
  });
});

describe("the feedback sender follows the server contract", () => {
  const payload = { reportId: "r_0123456789ab", category: "bug" as const, text: "t", context: { a: 1 }, local: LOCAL };
  const reply = (status: number, body: unknown = {}) =>
    vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(body), { status }));

  it("posts report_id at the top level and the replay data in context.local, no header", async () => {
    const fetchImpl = reply(200, { id: "fb_1", duplicate: false });
    const result = await httpFeedbackSender(fetchImpl as unknown as typeof fetch)(payload);
    expect(result).toEqual({ ok: true, serverId: "fb_1" });
    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("/api/feedback");
    expect(Object.keys(init!.headers as Record<string, string>)).toEqual(["content-type"]);
    const body = JSON.parse(init!.body as string);
    expect(body).toEqual({
      category: "bug",
      text: "t",
      context: { a: 1, local: LOCAL },
      report_id: "r_0123456789ab",
    });
    expect(body.context.offline).toBeUndefined();
  });

  it("counts duplicate:true as sent", async () => {
    const result = await httpFeedbackSender(reply(200, { id: "fb_1", duplicate: true }) as unknown as typeof fetch)(payload);
    expect(result).toEqual({ ok: true, serverId: "fb_1" });
  });

  it("retries 429 and 5xx, gives up on 400, retries a network error", async () => {
    for (const status of [429, 500, 503])
      expect(await httpFeedbackSender(reply(status) as unknown as typeof fetch)(payload)).toMatchObject({ ok: false, retryable: true });
    expect(await httpFeedbackSender(reply(400) as unknown as typeof fetch)(payload)).toMatchObject({ ok: false, retryable: false });
    const down = vi.fn(async () => { throw new TypeError("Failed to fetch"); });
    expect(await httpFeedbackSender(down as unknown as typeof fetch)(payload)).toMatchObject({ ok: false, retryable: true });
  });
});
