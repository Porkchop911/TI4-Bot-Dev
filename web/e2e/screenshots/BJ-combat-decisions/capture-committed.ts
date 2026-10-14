import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { artifactDir } from "../_shared/shot";

// These shots are not rendered by the screenshots runner: they are taken from real engine games by
// capture-real.sh (smoke run with TI4_SMOKE_SHOT_SUBTYPES), which needs a backend and minutes per
// game. The runner only checks that the committed PNGs listed in manifest.json are present and
// have the viewport sizes, so `npm run screenshots -- BJ` can build index.html from them.
test("committed real-game shots are present at desktop and phone size", ({}, testInfo) => {
  const folder = artifactDir(testInfo);
  const manifest = JSON.parse(readFileSync(join(folder, "manifest.json"), "utf8")) as {
    shots: { file: string }[];
  };
  for (const { file } of manifest.shots) {
    const path = join(folder, file);
    expect(existsSync(path), file).toBe(true);
    const png = readFileSync(path);
    const width = png.readUInt32BE(16);
    expect(width, file).toBe(file.includes("-phone") ? 390 : 1440);
  }
});
