import React, { createContext, useContext } from "react";
import type { LobbyDto } from "../protocol/types.ts";
import { playerDisplay, seatStyle } from "./playerDisplay.ts";
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

/** The visible number and shape remain readable when color cannot be perceived. */
export const SeatBadge: React.FC<{ position: number }> = ({ position }) => {
  const { color, symbol } = seatStyle(position);
  return (
    <span className="seat-badge" style={{ borderColor: color }} aria-label={`Position ${position}`}>
      <svg aria-hidden="true" viewBox="0 0 24 24" width="20" height="20" focusable="false">
        <circle cx="12" cy="12" r="10" fill={color} stroke="#f8fafc" strokeWidth="1.5" />
        <text
          x="12"
          y="16"
          textAnchor="middle"
          fill={position === 8 ? "#fff" : "#0b1220"}
          fontSize="12"
          fontWeight="bold"
        >
          {symbol}
        </text>
      </svg>
      <span>{position}</span>
    </span>
  );
};
