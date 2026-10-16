import type { CSSProperties } from "react";

/**
 * Room kept under the map while a prompt pill is showing (the choice banner sits 24px above the
 * bottom edge and is up to ~75px tall), in px. At or below 720px wide the banner is outside the
 * map, so `.board-stage` drops the inset in CSS.
 */
export const BOARD_PROMPT_INSET = 108;
/** Room kept otherwise, for the collapsed event log bar that overlays the bottom edge. */
export const BOARD_IDLE_INSET = 48;
/** The toolbar and seat legend flow in `.board-chrome` above the stage; its row is at least this tall. */
export const BOARD_TOOLBAR_INSET = 56;

/**
 * The map's drawing area: everything below the toolbar row and above the bottom inset. The SVG
 * viewBox is fitted into this box (preserveAspectRatio meet), so "reset" and the initial view
 * never place a tile row under the chrome or the prompt.
 */
export const boardStageStyle = (hasPrompt: boolean): CSSProperties => ({
  flex: "1 1 auto",
  minHeight: 0,
  boxSizing: "border-box",
  paddingBottom: hasPrompt ? BOARD_PROMPT_INSET : BOARD_IDLE_INSET,
});

/** Seat markers drawn on the map, in SVG units (about px at 1:1): 18 is the floor, up from 12. */
export const COMMAND_TOKEN_RADIUS = 11;
export const COMMAND_TOKEN_STEP = 23;
/** Command tokens sit this far below the tile centre (the hex narrows to a point at +70). */
export const COMMAND_TOKEN_Y = 60;
/** Control glyph next to a planet, up from 12. */
export const CONTROL_SYMBOL_FONT_SIZE = 20;
/** The "N units" badge under the planets: top offset, height, width and font size. */
export const UNITS_BADGE = { top: 34, height: 16, width: 62, fontSize: 11 };

/**
 * Planet discs on a map tile (pointy-top hex, radius 70, in SVG units), as offsets from the tile
 * centre. The value pill hangs on the disc's lower edge, so the lowest pill ends at about +33 and
 * the "N units" badge (+36) stays clear of it. Discs were r15 with 8px values.
 */
export interface PlanetSlot {
  dx: number;
  dy: number;
  r: number;
}
export function planetSlots(count: number): PlanetSlot[] {
  if (count <= 1) return [{ dx: 0, dy: -2, r: 24 }];
  if (count === 2)
    return [
      { dx: -31, dy: -2, r: 21 },
      { dx: 31, dy: -2, r: 21 },
    ];
  // 3 or more: one on top, the rest in a row below (a 4th planet shares the lower row).
  const slots: PlanetSlot[] = [{ dx: 0, dy: -28, r: 17 }];
  const lower = count - 1;
  const step = lower > 1 ? 72 / (lower - 1) : 0;
  for (let i = 0; i < lower; i++) {
    slots.push({ dx: Math.round((i - (lower - 1) / 2) * step), dy: 8, r: 17 });
  }
  return slots;
}
/** Value text on the map, in SVG units: at least ~12 at the default zoom (was 8). */
export const PLANET_VALUE_FONT_SIZE = 12;
export const PLANET_VALUE_PILL_HEIGHT = 15;
/** Half-width of the hex across its middle band; pills are kept inside it. */
export const TILE_HALF_WIDTH = 58;
