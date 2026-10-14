import { createContext, useContext } from "react";

/**
 * The body of the phone menu sheet. On a phone the top chrome is one slim bar; the map's own controls
 * (view switch, zoom, who's who) live in this sheet. The sheet registers its element when it is open;
 * the board portals its controls into it, so the zoom and view state stay in the board. Without a
 * provider (desktop, tests) the board keeps its controls in its own toolbar row.
 */
export interface MobileMenuSlot {
  slot: HTMLElement | null;
  setSlot: (element: HTMLElement | null) => void;
}

export const MobileMenuSlotContext = createContext<MobileMenuSlot | null>(null);

export const useMobileMenuSlot = () => useContext(MobileMenuSlotContext);
