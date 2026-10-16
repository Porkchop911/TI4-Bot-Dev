import type { Locator, Page } from "@playwright/test";

export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  desktop: { width: 1280, height: 720 },
  "390x844": { width: 390, height: 844 },
  "360x740": { width: 360, height: 740 },
  "844x390": { width: 844, height: 390 },
} as const;
export type SizeName = keyof typeof SIZES;

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const overlaps = (a: Box, b: Box) =>
  a.x < b.x + b.width - 0.5 && b.x < a.x + a.width - 0.5 && a.y < b.y + b.height - 0.5 && b.y < a.y + a.height - 0.5;

/** Bounding box that must exist. */
export async function box(target: Locator): Promise<Box> {
  const b = await target.boundingBox();
  if (!b) throw new Error("no bounding box");
  return b;
}

/** True when the topmost element at the centre of `target` is `target` or inside it (nothing lies over it). */
export async function onTop(_page: Page, target: Locator): Promise<boolean> {
  return target.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!top && (el === top || el.contains(top));
  });
}
