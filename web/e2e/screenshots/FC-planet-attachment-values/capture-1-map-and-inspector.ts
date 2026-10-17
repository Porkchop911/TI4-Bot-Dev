import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { hoverShot } from "../_shared/hover";
import { open, SIZES } from "./scene";

// A boosted planet on the map: its value pill shows the raised number in green with a "+" and names the
// source in its tooltip. Lodor (#26) has Mining World, Quann (#25) Dyson Sphere, Jord (#18) the Core Mining law.
for (const s of SIZES) {
  test(`map pills ${s.label}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: s.width, height: s.height });
    await open(page);
    await shot(page, testInfo, `1-map-${s.label}`);
  });

  test(`inspector ${s.label}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: s.width, height: s.height });
    await open(page);
    await page.getByTestId("system-hex-26").click();
    const inspector = page.getByTestId("system-inspector");
    await inspector.waitFor();
    await shot(page, testInfo, `2-inspector-lodor-${s.label}`, s.label === "desktop" ? { of: inspector, pad: 8 } : {});
  });
}

test("inspector with two kinds boosted", async ({ page }, testInfo) => {
  await open(page);
  await page.getByTestId("system-hex-25").click();
  const inspector = page.getByTestId("system-inspector");
  await inspector.waitFor();
  await shot(page, testInfo, "2b-inspector-quann-desktop", { of: inspector, pad: 8 });
});

test("tile tooltip", async ({ page }, testInfo) => {
  await open(page);
  await hoverShot(page, testInfo, "3-tile-tooltip", page.getByTestId("system-hex-26"), {
    tooltip: page.getByTestId("system-tooltip"),
  });
});
