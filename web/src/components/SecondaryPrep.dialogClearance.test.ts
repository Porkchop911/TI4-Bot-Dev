import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "SecondaryPrep.css"), "utf8");
const host = readFileSync(resolve(__dirname, "SecondaryPrepHost.tsx"), "utf8");

describe("the stand-in question keeps clear of the preparing banner", () => {
  it("pads both kinds of full-screen question dialog (choice-dialog and choice-workflow-dialog), not just the workflow one", () => {
    expect(css).toMatch(/\.secondary-prep--preparing\) :is\(\.choice-workflow-dialog, \.choice-dialog\) \{[^}]*padding-top: var\(--prep-banner-bottom/);
    expect(css).toMatch(/padding-bottom: var\(--prep-pad-bottom/);
    expect(css).toMatch(/padding-right: var\(--prep-pad-right/);
  });
  it("the host publishes the bottom and right clearance and removes them again", () => {
    for (const name of ["--prep-pad-bottom", "--prep-pad-right"]) {
      expect(host).toContain(`setProperty("${name}"`);
      expect(host).toContain(`removeProperty("${name}")`);
    }
  });
  it("short desktop windows get the fold control too", () => {
    expect(css).toMatch(/@media \(max-width: 720px\), \(max-height: 640px\) \{\s*\.button\.secondary-prep__fold/);
  });
});
