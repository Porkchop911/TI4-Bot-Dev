import { useEffect, useRef } from "react";
import type { FeedbackSourceData } from "./context.ts";

/**
 * Where the feedback sheet gets its game context from. The mounted game screen (or the lobby)
 * registers a getter that is read only when the sheet opens, so nothing re-renders on updates.
 * A game getter wins over a lobby getter.
 */
type Getter = () => FeedbackSourceData;
const getters: { lobby: Getter | null; game: Getter | null } = { lobby: null, game: null };

export function currentFeedbackSource(): FeedbackSourceData {
  return (getters.game ?? getters.lobby)?.() ?? {};
}

export function useRegisterFeedbackSource(kind: "lobby" | "game", getter: Getter): void {
  const latest = useRef(getter);
  latest.current = getter;
  useEffect(() => {
    const read: Getter = () => latest.current();
    getters[kind] = read;
    return () => {
      if (getters[kind] === read) getters[kind] = null;
    };
  }, [kind]);
}
