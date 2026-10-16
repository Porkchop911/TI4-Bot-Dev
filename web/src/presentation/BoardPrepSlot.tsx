import { createContext, useContext } from "react";

/**
 * The row in the board's own toolbar strip where the "Prepare your secondary" chip lives. The board
 * registers its element; the secondary-prep host portals the chip into it, so the chip takes part in
 * the strip's layout (the map is fitted below it) and can never cover the toolbar, the seat legend or
 * the top tiles. Without a provider the host falls back to an overlay.
 */
export interface BoardPrepSlot {
  slot: HTMLElement | null;
  setSlot: (element: HTMLElement | null) => void;
}

export const BoardPrepSlotContext = createContext<BoardPrepSlot | null>(null);

export const useBoardPrepSlot = () => useContext(BoardPrepSlotContext);
