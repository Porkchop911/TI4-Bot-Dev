import { useEffect, useRef } from "react";
import type { HistoryStatus, PendingChoiceDto } from "../protocol/types.ts";
import { isDryNonce } from "../presentation/dryChoice.ts";
import { i48sDeclineOption } from "../presentation/i48sOffer.ts";
import { readAskI48s } from "./useI48sSetting.ts";

/** Pause before answering, so the board paints and a double render settles. */
export const I48S_DECLINE_DELAY_MS = 400;

export interface I48sAutoDeclineInput {
  choice: PendingChoiceDto | null;
  /** The viewer's own seat; absent for spectators, who never act. */
  viewerSeat?: string | null;
  history?: HistoryStatus;
  busy?: boolean;
  submit: (optionId: string, nonce: string) => Promise<void>;
  onNotice?: (note: { id: string; text: string }) => void;
}

/**
 * With the "ask about I48S" switch off, declines the L1Z1X agent's offer for the viewer's own seat.
 * The engine still asks and the decline is an ordinary recorded `submit_choice`. Same "armed" rule
 * as the lone-move auto-submit: only when the game moved forward to the offer, never after an undo,
 * redo, restore or page load, so the player can always undo past it and see it again.
 */
export function useI48sAutoDecline({ choice, viewerSeat, history, busy, submit, onNotice }: I48sAutoDeclineInput) {
  const armed = useRef(false);
  const prev = useRef<HistoryStatus | null>(null);
  const evaluated = useRef<string | null>(null);
  const submitRef = useRef(submit);
  submitRef.current = submit;
  const noticeRef = useRef(onNotice);
  noticeRef.current = onNotice;
  const busyRef = useRef(busy);
  busyRef.current = busy;

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
  const declineId = !isDryNonce(nonce) ? i48sDeclineOption(choice, viewerSeat) : null;

  useEffect(() => {
    if (!nonce || evaluated.current === nonce) return;
    evaluated.current = nonce;
    const wasArmed = armed.current;
    armed.current = false;
    // Decided when the offer appears: switching off while it is open leaves this one to the player.
    if (!wasArmed || !declineId || readAskI48s()) return;
    const timer = window.setTimeout(() => {
      if (busyRef.current || readAskI48s()) return;
      noticeRef.current?.({ id: `i48s-${nonce}`, text: "I48S skipped (switched off)" });
      submitRef.current(declineId, nonce).catch(() => {
        // The decision stays open for a click.
      });
    }, I48S_DECLINE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [nonce, declineId]);
}
