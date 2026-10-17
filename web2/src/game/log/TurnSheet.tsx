import { useRef } from "react";
import type { ActionView, InspectView, Intent } from "../../model";
import { Button, cx, PhoneScope, ReadOnlyScope, usePhoneScreen } from "../../ui";
import { ActionPanel } from "../action/ActionPanel";
import { GameProvider, useDispatch, useSeats } from "../context";

/**
 * Whether the viewer's open decision came while the history mode was on: a question that was
 * open when the mode began is not new; a new question (`question` changes), or one where there was
 * none, is, until the mode ends.
 */
export function useFreshQuestion(history: boolean, asked: boolean, question: string): boolean {
  const from = useRef<string | null | undefined>(undefined);
  if (!history) {
    from.current = undefined;
  } else if (from.current === undefined) {
    from.current = asked ? question : null;
  }
  return history && asked && question !== from.current;
}

/** The heading of a decision, as its minimized line says it. */
export const decisionHeading = (view: ActionView) =>
  view.kind === "tactical" ? view.task.title : view.heading;

/**
 * The bar of the viewer's open decision while the screen is in the history mode: no control of
 * the decision is shown, only that it waits and the way back. A question that came while the
 * screen showed the past has the blinking dot of a pane tab. Desktop has it under the sheet of the
 * turn, a phone in the place of the footer.
 */
export function WaitingBar({
  heading,
  fresh,
  onBack,
  edge = true,
}: {
  heading: string;
  /** A line above it: under a pane. Not in a frame of its own. */
  edge?: boolean;
  /** The question came while the screen was in the history mode. */
  fresh: boolean;
  onBack: () => void;
}) {
  return (
    <div
      role="region"
      aria-label="Your decision"
      className={cx(
        "flex min-w-0 items-center gap-2.5 bg-surface px-3 py-2 touch:min-h-12",
        edge && "border-t border-line",
      )}
    >
      {fresh && (
        <>
          <span
            aria-hidden
            data-mark=""
            className="inline-block size-2 shrink-0 rounded-full bg-accent shadow-[0_0_6px_var(--accent)] animate-blink motion-reduce:animate-none"
          />
          <span className="sr-only">New:</span>
        </>
      )}
      <span className="min-w-0 flex-1 truncate text-sm" title={heading}>
        <span className="text-muted">Your decision is waiting: </span>
        <strong className="font-strong">{heading}</strong>
      </span>
      <Button tone="quiet" size="sm" onClick={onBack}>
        Back
      </Button>
    </div>
  );
}

/**
 * A turn or a phase of the Log, opened: its action as Live showed it, read-only, in the place of
 * the step sheet, over the board as it was then. The screen is in the history mode: the frame of
 * the board has the colour of the mode, a stamp on the board says which point it shows, and the
 * heading row names the turn with "‹" and "›" to the rows of the Log around it. Its step tabs are
 * a view of the turn; a row and a token show what was done and are no control, the inspector of a
 * system stays. "×" closes it. The viewer's open decision is not shown: its bar says that it
 * waits, and "Back" closes the turn and gives it back.
 */
export function TurnSheet({
  view,
  decision,
  fresh,
  status,
}: {
  view: InspectView;
  /** The viewer's open decision. Null: none. */
  decision: ActionView | null;
  /** The decision came while the screen was in the history mode. */
  fresh: boolean;
  status: string;
}) {
  const phone = usePhoneScreen();
  const dispatch = useDispatch();
  const seats = useSeats();
  const close = () => dispatch({ type: "closeTurn" });
  // The tabs open the steps of the turn; what is on the board opens the inspector. Nothing else
  // of the turn is sent to the game.
  const read = (intent: Intent) => {
    if (intent.type === "selectStep") {
      return dispatch({ type: "inspectTurn", turn: view.turn, step: intent.step });
    }
    if (intent.type === "inspectSystem" || intent.type === "openReference") {
      return dispatch(intent);
    }
  };
  const go = (turn: string | null) => turn && dispatch({ type: "inspectTurn", turn });
  const step =
    "grid size-7 shrink-0 place-items-center rounded text-lg text-muted hover:text-text disabled:opacity-30 touch:size-9";
  const nav = (
    <nav aria-label="Turns of the Log" className="flex min-w-0 items-center">
      <button
        type="button"
        aria-label="Previous turn"
        title="Previous turn"
        disabled={!view.previous}
        onClick={() => go(view.previous)}
        className={step}
      >
        ‹
      </button>
      <span className="min-w-0 truncate text-sm font-strong text-accent" title={view.name}>
        {view.name}
      </span>
      <button
        type="button"
        aria-label="Next turn"
        title="Next turn"
        disabled={!view.next}
        onClick={() => go(view.next)}
        className={step}
      >
        ›
      </button>
    </nav>
  );
  const panel = (
    <ReadOnlyScope>
      <GameProvider seats={seats} dispatch={read}>
        <ActionPanel view={view.action} status={status} onClose={close} nav={nav} />
      </GameProvider>
    </ReadOnlyScope>
  );
  // The frame of the board in the colour of the mode, and the point of the game that it shows.
  const past = (
    <>
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 z-1 shadow-[inset_0_0_0_3px_var(--accent)]"
      />
      <p
        role="status"
        aria-label="Board shows"
        className="pointer-events-none absolute top-11 left-2.5 z-2 max-w-[calc(100%-20px)] truncate rounded-sm border border-accent/60 bg-surface/90 px-2 py-0.5 text-xs font-strong text-accent touch:top-13"
      >
        {view.stamp}
      </p>
    </>
  );
  const waiting = decision && (
    <WaitingBar heading={decisionHeading(decision)} fresh={fresh} onBack={close} edge={false} />
  );
  if (phone) {
    // A phone has the bar of the decision in the place of the footer, under the pane.
    return (
      <>
        {past}
        <div
          role="region"
          aria-label="Turn"
          data-covers-board=""
          className="absolute inset-x-0 bottom-0 z-2 flex max-h-[60%] flex-col border-t border-accent/60 bg-surface"
        >
          <div className="flex min-h-0 flex-1 flex-col overflow-auto">{panel}</div>
        </div>
      </>
    );
  }
  const frame =
    "flex w-[412px] flex-col overflow-hidden rounded-lg border border-accent/60 bg-surface shadow-[0_12px_30px_#0008]";
  return (
    <>
      {past}
      <div className="absolute right-3 bottom-3 z-2 flex max-h-[80%] flex-col gap-2">
        <div role="region" aria-label="Turn" className={`${frame} min-h-0 flex-1`}>
          <PhoneScope className="flex min-h-0 flex-1 flex-col">{panel}</PhoneScope>
        </div>
        {waiting && (
          <div className={`${frame} flex-none`}>
            <PhoneScope className="flex min-w-0 flex-col">{waiting}</PhoneScope>
          </div>
        )}
      </div>
    </>
  );
}
