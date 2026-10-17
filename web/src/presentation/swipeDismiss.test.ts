import { describe, expect, it } from "vitest";
import {
  SWIPE_DISMISS_PX,
  SWIPE_SLOP_PX,
  isDismissKey,
  isSwipe,
  swipeEnd,
  swipeOpacity,
} from "./swipeDismiss.ts";

describe("swipe to dismiss", () => {
  it("treats a tiny movement as a tap, not a swipe", () => {
    expect(isSwipe(SWIPE_SLOP_PX - 1)).toBe(false);
    expect(isSwipe(-(SWIPE_SLOP_PX - 1))).toBe(false);
    expect(isSwipe(SWIPE_SLOP_PX)).toBe(true);
    expect(isSwipe(-SWIPE_SLOP_PX)).toBe(true);
  });

  it("dismisses a slow drag only past the dismiss distance, towards the side it went", () => {
    expect(swipeEnd({ dx: -(SWIPE_DISMISS_PX - 1), elapsedMs: 2000 })).toBe("snap-back");
    expect(swipeEnd({ dx: -SWIPE_DISMISS_PX, elapsedMs: 2000 })).toBe("dismiss-left");
    expect(swipeEnd({ dx: SWIPE_DISMISS_PX, elapsedMs: 2000 })).toBe("dismiss-right");
    expect(swipeEnd({ dx: 0, elapsedMs: 10 })).toBe("snap-back");
  });

  it("dismisses a short flick, but not a short slow drag", () => {
    expect(swipeEnd({ dx: -40, elapsedMs: 60 })).toBe("dismiss-left");
    expect(swipeEnd({ dx: 40, elapsedMs: 60 })).toBe("dismiss-right");
    expect(swipeEnd({ dx: -40, elapsedMs: 600 })).toBe("snap-back");
    // Too short even for a flick.
    expect(swipeEnd({ dx: -10, elapsedMs: 5 })).toBe("snap-back");
  });

  it("fades the toast while it is dragged away, never to invisible", () => {
    expect(swipeOpacity(0)).toBe(1);
    expect(swipeOpacity(-SWIPE_DISMISS_PX)).toBeLessThan(swipeOpacity(-20));
    expect(swipeOpacity(5000)).toBeGreaterThanOrEqual(0.2);
  });

  it("knows the keys that dismiss a focused toast", () => {
    for (const key of ["Escape", "Delete", "Backspace"]) expect(isDismissKey(key)).toBe(true);
    for (const key of ["a", "Tab", "ArrowLeft"]) expect(isDismissKey(key)).toBe(false);
  });
});
