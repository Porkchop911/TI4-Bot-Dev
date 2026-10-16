import type { LobbyDto } from "../protocol/types.ts";

/**
 * Fixed physical-position palette (TI4 traditional hues); neither slot IDs nor player IDs determine
 * these colors. Ordered so the first N seats are the most distinguishable subset for an N-player
 * game, with blue and green first and the two red-family hues (red 4, orange 6) kept apart:
 * blue, green, white, red, pink, orange, yellow, purple. Symbols stay the second channel.
 */
export const SEAT_COLORS = [
  "#3B82F6", // blue
  "#4ADE80", // green
  "#F1F5F9", // white
  "#EF4444", // red
  "#F472B6", // pink
  "#F97316", // orange
  "#F5D90A", // yellow
  "#A855F7", // purple
] as const;
export const SEAT_INK_DARK = "#0b1220";
export const SEAT_INK_LIGHT = "#ffffff";

function relativeLuminance(hex: string): number {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!m) return 0;
  const [r, g, b] = m.slice(1).map((part) => {
    const c = parseInt(part, 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Dark or white ink, whichever reads better on a seat-colored fill (white seat, yellow, green need dark). */
export function seatInk(fill: string): string {
  return contrastRatio(fill, SEAT_INK_DARK) >= contrastRatio(fill, SEAT_INK_LIGHT)
    ? SEAT_INK_DARK
    : SEAT_INK_LIGHT;
}
export const SEAT_SYMBOLS = ["●", "▲", "■", "◆", "★", "✚", "⬟", "◖"] as const;

export interface ParticipantDisplay {
  label: string;
  position: number | null;
  color: string;
  symbol: string;
}

export function seatStyle(position: number): Pick<ParticipantDisplay, "color" | "symbol"> {
  if (!Number.isInteger(position) || position < 1 || position > 8)
    return { color: "#94a3b8", symbol: "?" };
  return { color: SEAT_COLORS[position - 1], symbol: SEAT_SYMBOLS[position - 1] };
}

/** Resolve from the current roster; the started engine order is only a fallback for position. */
export function playerDisplay(
  lobby: LobbyDto | null,
  seatingOrder: readonly string[],
  id: string | null | undefined,
): ParticipantDisplay {
  if (!id) return { label: "Unknown participant", position: null, color: "#94a3b8", symbol: "?" };
  const slot = lobby?.slots.find((entry) => entry.occupant === id);
  const index = seatingOrder.indexOf(id);
  const position = slot?.position ?? (index >= 0 ? index + 1 : null);
  const sameName =
    slot?.nickname && lobby?.slots.filter((entry) => entry.nickname === slot.nickname).length !== 1;
  const style = seatStyle(position ?? 0);
  const label = slot?.nickname
    ? `${slot.nickname}${sameName ? ` (${style.symbol} Position ${position})` : ""}`
    : position
      ? `Participant at position ${position}`
      : "Unknown participant";
  return { label, position, ...style };
}
