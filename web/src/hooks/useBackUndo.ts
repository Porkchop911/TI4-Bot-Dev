import { useEffect, useRef, useState } from "react";

const GUARD = "ti4BackUndo";

export interface BackUndoOptions {
  /** Only single-player (offline) games. */
  enabled: boolean;
  /** The engine can take the last answer back right now. */
  canUndo: boolean;
  busy: boolean;
  undo: () => Promise<boolean>;
}

/**
 * The browser / system Back button as "undo my last decision" in a single-player game.
 *
 * One sentinel history entry (same URL) sits on top of the game's entry. Back pops it: with
 * something to undo the answer is taken back and the sentinel is pushed again, so the player
 * stays on the game screen; with nothing to undo we go back once more, so Back leaves the game
 * for the previous page as usual (never a trap; the game is autosaved either way). A reload keeps
 * the sentinel in the entry it was on, so none is ever pushed twice. Forward onto a sentinel is
 * ignored: it never redoes anything. Returns a short note to show (and announce) after an undo.
 */
export function useBackUndo({ enabled, canUndo, busy, undo }: BackUndoOptions): string {
  const latest = useRef({ canUndo, busy, undo });
  latest.current = { canUndo, busy, undo };
  const [note, setNote] = useState("");

  useEffect(() => {
    if (!enabled) return;
    const isGuard = (state: unknown) => Boolean(state && (state as Record<string, unknown>)[GUARD]);
    if (!isGuard(window.history.state)) window.history.pushState({ [GUARD]: true }, "", window.location.href);
    let timer: number | undefined;
    const onPop = (event: PopStateEvent) => {
      if (isGuard(event.state)) return; // forward onto the sentinel: nothing to do
      const { canUndo: can, busy: isBusy, undo: run } = latest.current;
      if (can && !isBusy) {
        window.history.pushState({ [GUARD]: true }, "", window.location.href);
        void run().then((done) => {
          setNote(done ? "Undid your last decision (Back)" : "Could not undo your last decision");
          window.clearTimeout(timer);
          timer = window.setTimeout(() => setNote(""), 4000);
        });
      } else if (isBusy) {
        // A history change is running: keep the player here, they can press Back again.
        window.history.pushState({ [GUARD]: true }, "", window.location.href);
      } else {
        // Nothing to undo: Back means leave, as it would without this hook.
        window.history.back();
      }
    };
    window.addEventListener("popstate", onPop);
    return () => {
      window.removeEventListener("popstate", onPop);
      window.clearTimeout(timer);
    };
  }, [enabled]);

  return note;
}
