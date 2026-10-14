import { useEffect, useRef, useState } from "react";
import type { HistoryStatus, PendingChoiceDto } from "../protocol/types.ts";
import { loneAction } from "../presentation/loneChoice.ts";
import { isDryNonce } from "../presentation/dryChoice.ts";
import { readAutoSubmitLone } from "./useAutoSubmitSetting.ts";

/** Pause before submitting, so the board paints and a double render or second tab settles. */
export const LONE_SUBMIT_DELAY_MS = 400;
const CLAIM_KEY = "ti4_lone_submit_claim";

export interface LoneAutoSubmitInput {
  choice: PendingChoiceDto | null;
  /** The viewer's own seat; absent for spectators, who never act. */
  viewerSeat?: string | null;
  history?: HistoryStatus;
  /** The pipeline runner or a history change is busy: leave the decision alone. */
  busy?: boolean;
  /** Same path a click takes (`submit_choice` with nonce and expected version). */
  submit: (optionId: string) => Promise<void>;
  onNotice?: (note: { id: string; text: string }) => void;
}

/** Cross-tab claim so two tabs of one seat do not both send; the server still rejects a repeat. */
function claim(nonce: string): boolean {
  try {
    if (localStorage.getItem(CLAIM_KEY) === nonce) return false;
    localStorage.setItem(CLAIM_KEY, nonce);
  } catch {
    // No storage: rely on the server's nonce and version checks.
  }
  return true;
}

/**
 * Answers a lone strategic action, lone system activation or movement with nothing to move for the viewer's own seat.
 * The engine still asks and the answer is an ordinary recorded `submit_choice`.
 *
 * "Armed" rule (undo safety): a lone decision is answered only when the game moved FORWARD to it
 * (the history cursor grew, with nothing to redo before or after) since the previous decision was
 * evaluated. A page load, a reconnect that changed nothing, an undo, a redo and a restore never arm
 * it, so a decision reached by undoing is always asked and the player can undo past it again.
 * Each nonce is evaluated once; a failed submit is not retried (the player answers by hand).
 */
export function useLoneAutoSubmit({
  choice,
  viewerSeat,
  history,
  busy,
  submit,
  onNotice,
}: LoneAutoSubmitInput) {
  const armed = useRef(false);
  const prev = useRef<HistoryStatus | null>(null);
  const evaluated = useRef<string | null>(null);
  // The nonce of a lone decision the client is about to answer and whose dialog has nothing to
  // show (movement with no movable unit): the shell keeps that dialog closed meanwhile.
  const [answering, setAnswering] = useState<string | null>(null);
  const submitRef = useRef(submit);
  submitRef.current = submit;
  const noticeRef = useRef(onNotice);
  noticeRef.current = onNotice;
  const busyRef = useRef(busy);
  busyRef.current = busy;

  // Track history: forward progress arms, anything else disarms.
  useEffect(() => {
    if (!history) return;
    const before = prev.current;
    prev.current = history;
    if (!before) return;
    if (history.generation !== before.generation) armed.current = false;
    else if (history.redo_count > 0 || before.redo_count > 0) armed.current = false;
    else if (history.cursor > before.cursor) armed.current = true;
    else if (history.cursor < before.cursor) armed.current = false;
  }, [history]);

  const nonce = choice?.nonce ?? null;
  // A client-made stand-in (secondary preparation) is never answered here; only the engine's own
  // decisions are.
  const lone = viewerSeat && choice?.actor === viewerSeat && !isDryNonce(nonce) ? loneAction(choice) : null;
  const optionId = lone?.optionId ?? null;
  const text = lone?.text ?? null;
  const hidesDialog = lone?.kind === "movement";

  useEffect(() => {
    if (!nonce || evaluated.current === nonce) return;
    evaluated.current = nonce;
    const wasArmed = armed.current;
    armed.current = false;
    if (!wasArmed || !optionId || text === null) return;
    if (hidesDialog && readAutoSubmitLone()) setAnswering(nonce);
    const timer = window.setTimeout(() => {
      if (busyRef.current || !readAutoSubmitLone() || !claim(nonce)) {
        setAnswering(null);
        return;
      }
      noticeRef.current?.({ id: `lone-${nonce}`, text });
      submitRef.current(optionId).catch(() => {
        // The session client already records the error; the decision stays open for a click.
        setAnswering(null);
      });
    }, LONE_SUBMIT_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [nonce, optionId, text, hidesDialog]);

  return { answeringNonce: answering };
}
