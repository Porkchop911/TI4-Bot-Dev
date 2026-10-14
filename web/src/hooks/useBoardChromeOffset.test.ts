import { describe, expect, it } from "vitest";
import { chromeBottomWithin } from "./useBoardChromeOffset.ts";

describe("chromeBottomWithin", () => {
  it("is measured from the container, not the viewport", () => {
    // header 120 px tall, toolbar row 56 px: the panel belongs 56 px below the board's top, not 176 px.
    expect(chromeBottomWithin(176, 120)).toBe(56);
  });
  it("ignores scroll (both rects move together) and rounds up", () => {
    expect(chromeBottomWithin(-24 + 56.2, -24)).toBe(57);
  });
  it("never goes negative", () => {
    expect(chromeBottomWithin(10, 40)).toBe(0);
  });
});
