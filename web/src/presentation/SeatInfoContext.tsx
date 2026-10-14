import React, { createContext, useContext, useMemo } from "react";
import type { PlayerView } from "../protocol/types.ts";

export interface SeatInfo {
  faction: string | null;
  technologies: readonly string[];
}

interface SeatInfoValue {
  players: readonly PlayerView[];
  viewerSeat: string | null;
}

const EMPTY: SeatInfoValue = { players: [], viewerSeat: null };
const SeatInfoContext = createContext<SeatInfoValue>(EMPTY);

/**
 * Who sits where, for the unit and faction info cards: their faction decides which flagship, mech
 * and variants apply, and their technologies decide which unit upgrades are in play. All of it is
 * already public in the player views.
 */
export const SeatInfoProvider: React.FC<{
  players: readonly PlayerView[] | Record<string, PlayerView> | undefined;
  viewerSeat?: string | null;
  children: React.ReactNode;
}> = ({ players, viewerSeat = null, children }) => {
  const list = useMemo(
    () => (Array.isArray(players) ? players : Object.values(players ?? {})),
    [players],
  );
  const value = useMemo(() => ({ players: list, viewerSeat }), [list, viewerSeat]);
  return <SeatInfoContext.Provider value={value}>{children}</SeatInfoContext.Provider>;
};

/** The faction and technologies of one seat (default: the viewing seat); nulls when unknown. */
export function useSeatInfo(seat?: string | null): SeatInfo {
  const { players, viewerSeat } = useContext(SeatInfoContext);
  const id = seat ?? viewerSeat;
  const player = id ? players.find((p) => p.id === id) : undefined;
  return { faction: player?.faction ?? null, technologies: player?.technologies ?? [] };
}
