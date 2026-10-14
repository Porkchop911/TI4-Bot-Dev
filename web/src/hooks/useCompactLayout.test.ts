import { describe, expect, it } from "vitest";
import { isCompactLayout, type LayoutEnv } from "./useCompactLayout.ts";
// @ts-expect-error plain .mjs build plugin
import { holdsOnPhone, scopeSelector } from "../../postcss-compact-layout.mjs";

const env = (over: Partial<LayoutEnv>): LayoutEnv => ({
  narrowViewport: false,
  touchOnly: false,
  screenWidth: 1920,
  screenHeight: 1080,
  override: null,
  ...over,
});

describe("isCompactLayout", () => {
  const cases: Array<[string, Partial<LayoutEnv>, boolean]> = [
    ["phone portrait, mobile viewport", { narrowViewport: true, touchOnly: true, screenWidth: 390, screenHeight: 844 }, true],
    ["phone landscape, short viewport", { narrowViewport: true, touchOnly: true, screenWidth: 844, screenHeight: 390 }, true],
    ["phone in desktop-site mode (980 layout)", { touchOnly: true, screenWidth: 390, screenHeight: 844 }, true],
    ["phone in desktop-site mode, landscape", { touchOnly: true, screenWidth: 844, screenHeight: 390 }, true],
    ["foldable unfolded, desktop site", { touchOnly: true, screenWidth: 600, screenHeight: 700 }, true],
    ["iPad 820x1180", { touchOnly: true, screenWidth: 820, screenHeight: 1180 }, false],
    ["iPad mini 744x1133", { touchOnly: true, screenWidth: 744, screenHeight: 1133 }, false],
    ["tablet landscape", { touchOnly: true, screenWidth: 1180, screenHeight: 820 }, false],
    ["desktop 1440x900 with mouse", { screenWidth: 1440, screenHeight: 900 }, false],
    ["narrow desktop window with mouse", { narrowViewport: true, screenWidth: 1440, screenHeight: 900 }, true],
    ["small screen with a mouse, wide window", { screenWidth: 500, screenHeight: 400 }, false],
    ["touch laptop (coarse + hover)", { screenWidth: 1366, screenHeight: 768 }, false],
    ["touch-only device with unknown screen size", { touchOnly: true, screenWidth: null, screenHeight: null }, false],
    ["override compact on desktop", { override: "compact" }, true],
    [
      "override desktop on a phone",
      { narrowViewport: true, touchOnly: true, screenWidth: 390, screenHeight: 844, override: "desktop" },
      false,
    ],
  ];
  for (const [name, e, expected] of cases) {
    it(`${name} -> ${expected ? "compact" : "desktop"}`, () => {
      expect(isCompactLayout(env(e))).toBe(expected);
    });
  }
});

describe("compact stylesheet duplication", () => {
  it("duplicates only width/height media that hold on a portrait phone", () => {
    expect(holdsOnPhone("(max-width: 720px)")).toBe(true);
    expect(holdsOnPhone("screen and (max-width: 720px), screen and (max-height: 500px)")).toBe(true);
    expect(holdsOnPhone("(max-width: 720px) and (min-height: 501px), (max-height: 500px)")).toBe(true);
    expect(holdsOnPhone("(max-height: 500px)")).toBe(false);
    expect(holdsOnPhone("(min-width: 721px)")).toBe(false);
    expect(holdsOnPhone("(pointer: coarse)")).toBe(false);
    expect(holdsOnPhone("(prefers-reduced-motion: reduce)")).toBe(false);
  });
  it("scopes selectors under the attribute without adding specificity", () => {
    expect(scopeSelector(".a .b")).toBe(':where(:root[data-layout-forced]) .a .b');
    expect(scopeSelector(":root:has(.x) .y")).toBe(':root[data-layout-forced]:has(.x) .y');
    expect(scopeSelector("html body")).toBe('html[data-layout-forced] body');
  });
});
