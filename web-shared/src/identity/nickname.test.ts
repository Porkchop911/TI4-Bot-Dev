import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";
import { validNickname } from "./nickname.ts";

// The cases that the server's `storage::validate_nickname` is checked against too.
const cases = JSON.parse(
  readFileSync(
    fileURLToPath(new URL("../../../fixtures/nickname/cases.json", import.meta.url)),
    "utf8",
  ),
) as { valid: string[]; invalid: string[] };

it("takes the names that the server takes", () => {
  expect(cases.valid.filter((name) => !validNickname(name))).toEqual([]);
});

it("refuses the names that the server refuses", () => {
  expect(cases.invalid.filter((name) => validNickname(name))).toEqual([]);
});

it("refuses what is no string, and a lone surrogate", () => {
  for (const value of [null, undefined, 3, {}, "x\ud800", "\udc00x"]) {
    expect(validNickname(value)).toBe(false);
  }
});
