import { afterEach, describe, expect, it, vi } from "vitest";
import {
  backoffDelay,
  describeError,
  fetchWithRetry,
  isNetworkError,
  isTransientResponse,
  isTransientStatus,
  onResume,
  ServerUnreachableError,
} from "./resilience.ts";

const res = (status: number, body = "") => new Response(body, { status });
const noSleep = () => Promise.resolve();

afterEach(() => {
  vi.useRealTimers();
});

describe("transient classification", () => {
  it("treats network failures and proxy gateway statuses as transient, nothing else", () => {
    expect(isNetworkError(new TypeError("Failed to fetch"))).toBe(true);
    expect(isNetworkError(new Error("Lobby request failed (500)"))).toBe(false);
    for (const status of [502, 503, 504]) expect(isTransientStatus(status)).toBe(true);
    for (const status of [200, 400, 403, 404, 409, 500]) expect(isTransientStatus(status)).toBe(false);
  });

  it("counts a bare 500 (the vite proxy's answer for a dead backend) but not a 500 with a message", async () => {
    expect(await isTransientResponse(res(500))).toBe(true);
    expect(await isTransientResponse(res(500, "engine panicked"))).toBe(false);
    expect(await isTransientResponse(res(409, ""))).toBe(false);
  });
});

describe("backoffDelay", () => {
  it("doubles per attempt, is jittered between half and full, and is capped", () => {
    expect(backoffDelay(1, { random: () => 0 })).toBe(500);
    expect(backoffDelay(1, { random: () => 0.999 })).toBeLessThanOrEqual(1_000);
    expect(backoffDelay(3, { random: () => 1 })).toBe(4_000);
    expect(backoffDelay(30, { random: () => 1 })).toBe(15_000);
    expect(backoffDelay(30, { random: () => 0 })).toBe(7_500);
  });
});

describe("fetchWithRetry", () => {
  it("returns the first response immediately when the network works", async () => {
    const send = vi.fn().mockResolvedValue(res(200));
    const sleep = vi.fn(noSleep);
    expect((await fetchWithRetry(send, { sleep })).status).toBe(200);
    expect(send).toHaveBeenCalledTimes(1);
    expect(sleep).not.toHaveBeenCalled();
  });

  it("retries network errors and 502/503/504, then returns the first real answer", async () => {
    const send = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockResolvedValueOnce(res(502))
      .mockResolvedValueOnce(res(504))
      .mockResolvedValueOnce(res(200));
    const sleeps: number[] = [];
    const response = await fetchWithRetry(send, {
      attempts: 5,
      random: () => 1,
      sleep: async (ms) => {
        sleeps.push(ms);
      },
    });
    expect(response.status).toBe(200);
    expect(sleeps).toEqual([1_000, 2_000, 4_000]);
  });

  it("does not retry 4xx, 409 or an ordinary 500: those are the server's real answer", async () => {
    for (const status of [400, 403, 404, 409]) {
      const send = vi.fn().mockResolvedValue(res(status));
      expect((await fetchWithRetry(send, { sleep: noSleep })).status).toBe(status);
      expect(send).toHaveBeenCalledTimes(1);
    }
    const send = vi.fn().mockResolvedValue(res(500, "boom"));
    expect((await fetchWithRetry(send, { sleep: noSleep })).status).toBe(500);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("does not swallow a non-network exception", async () => {
    const send = vi.fn().mockRejectedValue(new RangeError("bug"));
    await expect(fetchWithRetry(send, { sleep: noSleep })).rejects.toThrow(RangeError);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("throws ServerUnreachableError with plain wording after the last attempt", async () => {
    const send = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
    const failure = await fetchWithRetry(send, { attempts: 3, sleep: noSleep }).catch((e) => e);
    expect(failure).toBeInstanceOf(ServerUnreachableError);
    expect(failure.message).not.toMatch(/TypeError|Failed to fetch|lobby/i);
    expect(send).toHaveBeenCalledTimes(3);
  });

  it("stops retrying once cancelled", async () => {
    let cancelled = false;
    const send = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
    await expect(
      fetchWithRetry(send, {
        attempts: 10,
        cancelled: () => cancelled,
        sleep: async () => {
          cancelled = true;
        },
      }),
    ).rejects.toThrow(ServerUnreachableError);
    expect(send).toHaveBeenCalledTimes(1);
  });
});

describe("describeError", () => {
  it("never shows a raw TypeError", () => {
    expect(describeError(new TypeError("Failed to fetch"))).not.toMatch(/TypeError|Failed to fetch/);
    expect(describeError(new Error("Lobby ready failed (409): nope"))).toBe("Lobby ready failed (409): nope");
    expect(describeError(new Error("x"), "Join")).toBe("Join: x");
  });
});

describe("onResume", () => {
  it("collapses visibility, focus and pageshow into one call and reports how long the page was away", async () => {
    vi.useFakeTimers();
    let clock = 1_000;
    const listener = vi.fn();
    const stop = onResume(listener, () => clock);
    const setVisibility = (state: "hidden" | "visible") => {
      Object.defineProperty(document, "visibilityState", { configurable: true, value: state });
      document.dispatchEvent(new Event("visibilitychange"));
    };
    setVisibility("hidden");
    clock += 30 * 60_000;
    setVisibility("visible");
    window.dispatchEvent(new Event("focus"));
    const show = new Event("pageshow") as PageTransitionEvent;
    Object.defineProperty(show, "persisted", { value: true });
    window.dispatchEvent(show);
    await vi.advanceTimersByTimeAsync(10);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith({ reason: "pageshow", awayMs: 30 * 60_000 });
    window.dispatchEvent(new Event("online"));
    await vi.advanceTimersByTimeAsync(10);
    expect(listener).toHaveBeenLastCalledWith(expect.objectContaining({ reason: "online" }));
    stop();
    window.dispatchEvent(new Event("online"));
    await vi.advanceTimersByTimeAsync(10);
    expect(listener).toHaveBeenCalledTimes(2);
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  });
});
