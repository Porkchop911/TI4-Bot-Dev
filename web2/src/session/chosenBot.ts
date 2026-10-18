// The bot that "+ Bot" and "Fill" of the lobby put in a seat: a choice of this browser, kept for
// the next table, not a part of the table.
import { useCallback, useState } from "react";

const CHOSEN_BOT_KEY = "ti4.lobby.bot";

/** The kind that this browser chose last (null: none yet), and the setter that keeps it. */
export function useChosenBot(): [string | null, (kind: string) => void] {
  const [bot, setBot] = useState<string | null>(() => localStorage.getItem(CHOSEN_BOT_KEY));
  const choose = useCallback((kind: string) => {
    localStorage.setItem(CHOSEN_BOT_KEY, kind);
    setBot(kind);
  }, []);
  return [bot, choose];
}
