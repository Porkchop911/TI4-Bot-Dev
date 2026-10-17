import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "AutoResolveToast.css"), "utf8");
const bar = readFileSync(resolve(__dirname, "TurnActionBar.css"), "utf8");

// Runs o1-02-2032, o3-05-2038 (phone): the corner toasts sat on the open turn menu sheet and
// covered its Close button and entries.
describe("corner toasts keep clear of a phone's open turn menu", () => {
  it("the turn menu is a full sheet on a phone", () => {
    expect(bar).toMatch(/@media \(max-width: 720px\) \{[^@]*\.turn-bar__menu,\s*\.turn-bar__menu--trade \{\s*position: fixed;/);
  });
  it("hides the toast column while that sheet is open", () => {
    expect(css).toMatch(
      /@media \(max-width: 720px\) \{\s*:root:has\(\.turn-bar__menu\) \.auto-resolve-toast-container \{\s*visibility: hidden;/,
    );
  });
});
