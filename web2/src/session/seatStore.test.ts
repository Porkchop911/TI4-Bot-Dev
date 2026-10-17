import { afterEach, describe, expect, it, vi } from "vitest";
import { STAGED_DEBOUNCE_MS, seatSync } from "./seatStore";

afterEach(() => {
  vi.useRealTimers();
});

describe("seatSync", () => {
  it("takes an echo that comes within the write, as the engine of a local game sends it", () => {
    vi.useFakeTimers();
    const writes: unknown[] = [];
    let revision = 0;
    const sync = seatSync((slot, base, value) => {
      expect(base).toBe(revision);
      writes.push(value);
      revision += 1;
      // The engine answers before `put` returns.
      sync.receive({ revision, slots: { [slot]: { revision, value } }, writer: 1 });
      return true;
    });
    sync.receive({ revision: 0, slots: {}, connection: 1 });

    sync.store.set("plan", { cursor: 0 });
    vi.advanceTimersByTime(STAGED_DEBOUNCE_MS);
    // The next change is written at once, not after the wait for an answer.
    sync.store.set("plan", { cursor: 1 });
    vi.advanceTimersByTime(STAGED_DEBOUNCE_MS);
    sync.store.set("plan", { cursor: 2 });
    sync.flushNow();

    expect(writes).toEqual([{ cursor: 0 }, { cursor: 1 }, { cursor: 2 }]);
    expect(sync.waiting()).toBe(false);
    sync.close();
  });
});
