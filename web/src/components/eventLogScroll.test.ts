import { describe, expect, it } from "vitest";
import {
  PIN_THRESHOLD_PX,
  followTarget,
  isPinned,
  logHistoryKey,
  nextUnseen,
  restoreTarget,
} from "./eventLogScroll.ts";

const m = (scrollTop: number, scrollHeight = 1000, clientHeight = 320) => ({
  scrollTop,
  scrollHeight,
  clientHeight,
});

describe("event log scroll decisions", () => {
  it("is pinned within the threshold of the bottom, not above it", () => {
    expect(isPinned(m(680))).toBe(true);
    expect(isPinned(m(680 - PIN_THRESHOLD_PX))).toBe(true);
    expect(isPinned(m(680 - PIN_THRESHOLD_PX - 1))).toBe(false);
    expect(isPinned(m(0))).toBe(false);
    // Content shorter than the box is trivially pinned.
    expect(isPinned(m(0, 200, 320))).toBe(true);
  });

  it("follows new content only while pinned", () => {
    // 8 events (~208px) arrive in one burst: the new bottom is 888.
    expect(followTarget(true, m(680, 1208))).toBe(888);
    expect(followTarget(false, m(120, 1208))).toBeUndefined();
    // A new round/phase node opening while pinned is just more content.
    expect(followTarget(true, m(680, 1500))).toBe(1180);
  });

  it("counts unseen events only while scrolled up and clears at the bottom", () => {
    expect(nextUnseen(0, false, 70, 71)).toBe(1);
    expect(nextUnseen(1, false, 71, 79)).toBe(9);
    expect(nextUnseen(3, true, 79, 80)).toBe(0);
    // A shorter history (undo) neither adds nor clears.
    expect(nextUnseen(2, false, 80, 75)).toBe(2);
  });

  it("resets manual expansion by history generation, not by reconnect", () => {
    const a = [] as unknown[];
    const b = [] as unknown[];
    expect(logHistoryKey(3, a)).toBe(logHistoryKey(3, b)); // reconnect: same generation
    expect(logHistoryKey(3, a)).not.toBe(logHistoryKey(4, a)); // undo/redo/restore
    expect(logHistoryKey(undefined, a)).toBe(a); // old server: identity fallback
    expect(logHistoryKey(undefined, a)).not.toBe(logHistoryKey(undefined, b));
  });

  it("restores the hidden list to the bottom when pinned, else to the saved position (clamped)", () => {
    expect(restoreTarget(true, 150, m(0, 1208))).toBe(888);
    expect(restoreTarget(false, 150, m(0, 1208))).toBe(150);
    expect(restoreTarget(false, 900, m(0, 600))).toBe(280); // history got shorter
    expect(restoreTarget(false, 50, m(0, 100, 320))).toBe(0);
  });
});
