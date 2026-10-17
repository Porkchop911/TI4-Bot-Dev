import React, { createContext, useContext } from "react";
import type { LobbyDto } from "../protocol/types.ts";
import { playerDisplay, seatInk, seatStyle } from "./playerDisplay.ts";
import { participantReferences, participantText } from "./participantText.ts";

const PlayerContext = createContext<{ lobby: LobbyDto | null; seatingOrder: readonly string[] }>({
  lobby: null,
  seatingOrder: [],
});

export const PlayerIdentityProvider: React.FC<{
  lobby: LobbyDto | null;
  seatingOrder: readonly string[];
  children: React.ReactNode;
}> = ({ lobby, seatingOrder, children }) => (
  <PlayerContext.Provider value={{ lobby, seatingOrder }}>{children}</PlayerContext.Provider>
);

export function usePlayerIdentity() {
  const { lobby, seatingOrder } = useContext(PlayerContext);
  return (id: string | null | undefined) => playerDisplay(lobby, seatingOrder, id);
}

/** Presentation only: never feed this text back into protocol or choice IDs. */
export function useParticipantText() {
  const { lobby, seatingOrder } = useContext(PlayerContext);
  return (text: string) => participantText(text, lobby, seatingOrder);
}

export function useParticipantParts() {
  const { lobby, seatingOrder } = useContext(PlayerContext);
  return (text: string) =>
    participantReferences(text, lobby).map((part) =>
      typeof part === "string" ? part : playerDisplay(lobby, seatingOrder, part.id),
    );
}

/**
 * Whether an option id names a seat at this table (an elect-a-player vote, Committee Formation).
 * Presentation only: the option id still goes back to the engine unchanged.
 */
export function useIsSeat() {
  const { lobby, seatingOrder } = useContext(PlayerContext);
  return (id: string | null | undefined) =>
    !!id && (seatingOrder.includes(id) || !!lobby?.slots.some((slot) => slot.occupant === id));
}

/** Badge glyph size in px: 40% over the old 20, and never under 18 so the seat shape stays readable. */
export const SEAT_BADGE_SIZE = 28;

/** The visible number and shape remain readable when color cannot be perceived. */
export const SeatBadge: React.FC<{ position: number }> = ({ position }) => {
  const { color, symbol } = seatStyle(position);
  return (
    <span className="seat-badge" style={{ borderColor: color }} aria-label={`Position ${position}`}>
      <svg aria-hidden="true" viewBox="0 0 24 24" width={SEAT_BADGE_SIZE} height={SEAT_BADGE_SIZE} focusable="false">
        <circle cx="12" cy="12" r="10" fill={color} stroke="#f8fafc" strokeWidth="2" />
        <text
          x="12"
          y="17"
          textAnchor="middle"
          fill={seatInk(color)}
          fontSize="15"
          fontWeight="bold"
        >
          {symbol}
        </text>
      </svg>
      <span>{position}</span>
    </span>
  );
};
