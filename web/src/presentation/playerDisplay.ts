import type { LobbyDto } from "../protocol/types.ts";

/** Fixed physical-position palette; neither slot IDs nor player IDs determine these colors. */
export const SEAT_COLORS = [
  "#E69F00",
  "#56B4E9",
  "#009E73",
  "#F0E442",
  "#0072B2",
  "#D55E00",
  "#CC79A7",
  "#000000",
] as const;
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
