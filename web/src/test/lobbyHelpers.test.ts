import { describe, expect, it, vi } from "vitest";
import type { APIRequestContext } from "@playwright/test";
import { gameSnapshot } from "../../e2e/lobbyHelpers";

// The Playwright runtime does not load under jsdom; the helper only needs its `expect`.
vi.mock("@playwright/test", async () => ({ expect: (await import("vitest")).expect }));

/** A request context whose responses count how many bodies were read and how many were released. */
function fakeRequest(body: unknown, ok = true) {
  const counts = { gets: 0, disposed: 0 };
  const request = {
    get: async () => {
      counts.gets++;
      return {
        ok: () => ok,
        status: () => (ok ? 200 : 500),
        json: async () => body,
        text: async () => JSON.stringify(body),
        dispose: async () => {
          counts.disposed++;
        },
      };
    },
  } as unknown as APIRequestContext;
  return { request, counts };
}

describe("gameSnapshot", () => {
  // Playwright keeps every APIResponse body in the worker until it is disposed. The smoke harness
  // polls the snapshot several times per decision, so a long game grew the worker to 7 GB.
  it("releases the response body after reading it", async () => {
    const { request, counts } = fakeRequest({ game_version: 7 });
    const snap = await gameSnapshot(request, "g1", "s1");
    expect(snap.game_version).toBe(7);
    expect(counts.disposed).toBe(counts.gets);
  });

  it("releases a failed response too", async () => {
    const { request, counts } = fakeRequest({ error: "nope" }, false);
    await expect(gameSnapshot(request, "g1", "s1")).rejects.toThrow();
    expect(counts.disposed).toBe(counts.gets);
  });
});
