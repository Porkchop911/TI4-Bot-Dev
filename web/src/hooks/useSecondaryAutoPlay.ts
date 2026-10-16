import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { HistoryStatus, PendingChoiceDto } from "../protocol/types.ts";
import type { TokenStep } from "../presentation/commandTokens.ts";
import type { StepResolution } from "../presentation/secondaryPlan.ts";
import type { PaymentStep } from "../presentation/paymentDraft.ts";
import { readSecondaryPrepMode } from "./useSecondaryPrepMode.ts";

/** How long the toast is visible (and cancellable) before a prepared answer is sent. */
export const AUTO_PLAY_DELAY_MS = 2500;
/** After sending, how long the real decision UI stays held back before it opens anyway. */
export const AUTO_PLAY_HOLD_LIMIT_MS = 8000;
/** How long the "auto-played" notice stays after the answer went out. */
export const AUTO_PLAY_PLAYED_MS = 5000;
const CLAIM_KEY = "ti4_secondary_autoplay_claim";

export interface SecondaryAutoPlayInput {
  choice: PendingChoiceDto | null;
  /** The viewer's own seat; absent for spectators, who never act. */
  viewerSeat?: string | null;
  history?: HistoryStatus;
  /**
   * The event log only grew since the previous render (the old log is a prefix of the new one). A
   * committed batch replaces the live session and bumps the history generation while the game moves
   * forward; undo, redo and restore bump it too but rewrite the log. Only the former may arm.
   */
  logExtended?: boolean;
  /** The pipeline runner or a history change is busy: leave the decision alone. */
  busy?: boolean;
  /** What the prepared plan says about this decision (already validated against its options). */
  resolution: StepResolution;
  /** Same path a click takes (`submit_choice` with nonce and expected version). */
  submitOption: (optionId: string) => Promise<void>;
  /** Same path the token panel's confirm takes. */
  submitTokens?: (steps: TokenStep[]) => Promise<void>;
  /** Same path the production builder's "Confirm builds" takes (one batch, or its build queue). */
  submitProduction?: (units: string[], destination: string) => Promise<void>;
  /** Same path the payment drawer's confirm takes. */
  submitPayment?: (steps: PaymentStep[]) => Promise<void>;
  /** Same path Diplomacy's ready bar takes (one batch for both planets). */
  submitReady?: (planetIds: string[]) => Promise<void>;
}

export interface AutoPlayNotice {
  nonce: string;
  text: string;
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
 * Auto mode: sends the prepared answer for the viewer's own pending decision, once, after a short
 * visible delay in which it can be cancelled. The engine still asks and the answer is an ordinary
 * recorded submission, so nothing is spent before this decision is answered.
 *
 * The same "armed" rule as the lone-case auto-submit (undo safety): a decision is auto-answered
 * only when the game moved FORWARD to it (the history cursor grew with nothing to redo before or
 * after) since the previous decision was evaluated. A page load, a reconnect that changed nothing,
 * an undo, a redo and a restore never arm it, so a decision reached by undoing is always asked.
 * Each nonce is evaluated once; a failed submit is not retried. Multi-step plans (follow, then the
 * technology) are armed again by their own forward progress, and stop at the first step that does
 * not validate (`review`) or that the plan has nothing to say about (`none`).
 *
 * A generation change disarms too, except when it is a batch commit: the session was replaced, the
 * log only grew and the cursor moved forward (see `logExtended`). Otherwise every batch (a move, a
 * production, a token purchase by any seat) would turn a prepared answer into a manual question.
 */
export function useSecondaryAutoPlay({
  choice,
  viewerSeat,
  history,
  logExtended,
  busy,
  resolution,
  submitOption,
  submitTokens,
  submitProduction,
  submitPayment,
  submitReady,
}: SecondaryAutoPlayInput) {
  const armed = useRef(false);
  const prev = useRef<HistoryStatus | null>(null);
  const evaluated = useRef<string | null>(null);
  /** Whether the game moved forward to this nonce's decision, fixed when it first appears. */
  const armedFor = useRef<{ nonce: string; armed: boolean } | null>(null);
  const timer = useRef<number | null>(null);
  const [pending, setPending] = useState<AutoPlayNotice | null>(null);
  /** Nonce whose decision UI is held back while the answer is scheduled or in flight. */
  const [held, setHeld] = useState<string | null>(null);
  const holdTimer = useRef<number | null>(null);
  /** The last answer sent for the viewer, announced after the fact. */
  const [played, setPlayed] = useState<string | null>(null);
  const playedTimer = useRef<number | null>(null);
  const submitRef = useRef(submitOption);
  submitRef.current = submitOption;
  const tokensRef = useRef(submitTokens);
  tokensRef.current = submitTokens;
  const productionRef = useRef(submitProduction);
  productionRef.current = submitProduction;
  const paymentRef = useRef(submitPayment);
  paymentRef.current = submitPayment;
  const readyRef = useRef(submitReady);
  readyRef.current = submitReady;
  const busyRef = useRef(busy);
  busyRef.current = busy;
  const resolutionRef = useRef(resolution);
  resolutionRef.current = resolution;

  // Layout effects: the hold is decided before the browser paints, so the decision UI never flashes.
  // Track history: forward progress arms, anything else disarms.
  useLayoutEffect(() => {
    if (!history) return;
    const before = prev.current;
    prev.current = history;
    if (!before) return;
    if (history.generation !== before.generation) {
      armed.current =
        Boolean(logExtended) &&
        history.cursor > before.cursor &&
        history.redo_count === 0 &&
        before.redo_count === 0;
    } else if (history.redo_count > 0 || before.redo_count > 0) armed.current = false;
    else if (history.cursor > before.cursor) armed.current = true;
    else if (history.cursor < before.cursor) armed.current = false;
  }, [history]);

  const nonce = choice && viewerSeat && choice.actor === viewerSeat ? choice.nonce : null;
  const actionable =
    resolution.kind === "option" ||
    resolution.kind === "tokens" ||
    resolution.kind === "production" ||
    resolution.kind === "payment" ||
    resolution.kind === "ready";
  const text = actionable ? resolution.text : null;

  useLayoutEffect(() => {
    if (!nonce || evaluated.current === nonce) return;
    // The armed state belongs to the decision, taken when it first appears. A plan that resolves a
    // moment later (the planets of a payment, the prepared answer loading) still fires; a decision
    // that is never actionable is simply asked.
    if (armedFor.current?.nonce !== nonce) {
      armedFor.current = { nonce, armed: armed.current };
      armed.current = false;
    }
    if (!armedFor.current.armed) {
      evaluated.current = nonce;
      return;
    }
    if (!actionable || text === null) return;
    evaluated.current = nonce;
    if (readSecondaryPrepMode() !== "auto") return;
    setPending({ nonce, text });
    setHeld(nonce);
    // False once this decision's effect is torn down (the answer moved the game on): a late
    // settle must not arm a hold timer that would release a later decision's hold.
    let live = true;
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setPending(null);
      const current = resolutionRef.current;
      if (busyRef.current || readSecondaryPrepMode() !== "auto" || !claim(nonce)) {
        setHeld(null);
        return;
      }
      let sent: Promise<void> | undefined;
      if (current.kind === "option") sent = submitRef.current(current.optionId);
      else if (current.kind === "tokens") sent = tokensRef.current?.(current.steps);
      else if (current.kind === "production")
        sent = productionRef.current?.(current.units, current.destination);
      else if (current.kind === "payment") sent = paymentRef.current?.(current.steps);
      else if (current.kind === "ready") sent = readyRef.current?.(current.planetIds);
      if (!sent) {
        setHeld(null);
        return;
      }
      // A rejected or failed answer opens the decision at once; the session client records the error.
      // The hold stays for as long as the request runs (a slow batch commit takes many seconds): a
      // decision shown meanwhile is still open at the old version, and a click on it would send a
      // second answer that the server refuses as stale.
      const sentText = text;
      sent.then(
        () => {
          // Safety net: the decision UI opens if the answer was accepted but nothing moved on.
          if (live) holdTimer.current = window.setTimeout(() => setHeld(null), AUTO_PLAY_HOLD_LIMIT_MS);
          setPlayed(sentText);
          if (playedTimer.current !== null) window.clearTimeout(playedTimer.current);
          playedTimer.current = window.setTimeout(() => setPlayed(null), AUTO_PLAY_PLAYED_MS);
        },
        () => {
          if (live) setHeld(null);
        },
      );
    }, AUTO_PLAY_DELAY_MS);
    return () => {
      live = false;
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
      if (holdTimer.current !== null) window.clearTimeout(holdTimer.current);
      holdTimer.current = null;
      setPending(null);
      setHeld(null);
    };
  }, [nonce, actionable, text]);

  useEffect(
    () => () => {
      if (playedTimer.current !== null) window.clearTimeout(playedTimer.current);
    },
    [],
  );

  /** Stops the scheduled answer; the decision then stays open for a click. */
  const cancel = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    setPending(null);
    setHeld(null);
  }, []);

  return { pending, played, cancel, holding: held !== null && held === nonce };
}
