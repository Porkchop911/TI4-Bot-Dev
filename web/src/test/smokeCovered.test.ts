import { describe, expect, it } from "vitest";
import { coveredBy } from "../../e2e/smokePolicy.ts";

describe("coveredBy (smoke harness visibility)", () => {
  it("does not hide a Confirm button that is only scrolled past its dialog's edge", () => {
    // Run 03-2104: the Divert Funding dialog's options pushed Confirm half below the scrolling
    // panel; the point under its center hit the dialog backdrop, so the button was never offered.
    const dialog = document.createElement("div");
    const panel = document.createElement("div");
    const confirm = document.createElement("button");
    dialog.append(panel);
    panel.append(confirm);
    expect(coveredBy(confirm, dialog)).toBe(false);
    expect(coveredBy(confirm, panel)).toBe(false);
  });

  it("still hides a board control behind a modal, and keeps a hit on the control itself", () => {
    const board = document.createElement("div");
    const hex = document.createElement("button");
    const label = document.createElement("span");
    hex.append(label);
    board.append(hex);
    const modal = document.createElement("div");
    expect(coveredBy(hex, modal)).toBe(true);
    expect(coveredBy(hex, label)).toBe(false);
    expect(coveredBy(hex, null)).toBe(false);
  });
});
