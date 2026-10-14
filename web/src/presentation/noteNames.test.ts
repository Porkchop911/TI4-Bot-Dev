import { describe, expect, it } from "vitest";
import { GENERATED_CONTENT_CATALOG } from "../protocol/generatedContentManifest.ts";
import { printedCardName } from "./tradeNames.ts";

const NOTES = GENERATED_CONTENT_CATALOG.promissoryNotes as unknown as Record<
  string,
  { name: string; faction?: string }
>;

describe("promissory note names", () => {
  it("every faction note id the engine deals reads as its printed name", () => {
    const problems: string[] = [];
    for (const [alias, note] of Object.entries(NOTES)) {
      if (!note.faction) continue;
      const shown = printedCardName("note", `${alias}:${note.faction}`);
      if (!shown.known || !shown.name.startsWith(note.name)) problems.push(`${alias}: ${shown.name}`);
    }
    expect(problems).toEqual([]);
  });

  it("names the generic notes the engine deals to every seat", () => {
    for (const [alias, name] of [
      ["cf", "Ceasefire"],
      ["ps", "Political Secret"],
      ["ta", "Trade Agreement"],
      ["an", "Alliance"],
    ]) {
      expect(printedCardName("note", `${alias}:sol`).name).toBe(`${name} (sol)`);
    }
  });
});
