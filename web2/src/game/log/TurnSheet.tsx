import { useRef } from "react";
import type { ActionView, InspectView, Intent, LiveLineView } from "../../model";
import { Button, cx, Icon, PhoneScope, ReadOnlyScope, usePhoneScreen } from "../../ui";
import { ActionPanel } from "../action/ActionPanel";
import { MinimizedLine } from "../action/StepSheet";
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
  mark,
  onBack,
  edge = true,
}: {
  heading: string;
  /** The colour of the live workspace: the mark is of the live decision, not of the past. */
  mark: string;
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
            style={{ background: mark, boxShadow: `0 0 6px ${mark}` }}
            className="inline-block size-2 shrink-0 rounded-full animate-blink motion-reduce:animate-none"
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
 * The action of a turn of the Log, read-only: its step tabs open its steps, what is on the board
 * opens the inspector, and nothing else of it is sent to the game. "▾" minimizes it to one line,
 * as the step sheet; a tap on the line restores it.
 */
export function ReadOnlyTurn({
  view,
  status,
  minimized,
  onMinimized,
}: {
  view: InspectView;
  status: string;
  minimized: boolean;
  onMinimized: (minimized: boolean) => void;
}) {
  const dispatch = useDispatch();
  const seats = useSeats();
  const read = (intent: Intent) => {
    if (intent.type === "selectStep") {
      return dispatch({ type: "inspectTurn", turn: view.turn, step: intent.step });
    }
    if (intent.type === "inspectSystem" || intent.type === "openReference") {
      return dispatch(intent);
    }
  };
  return minimized ? (
    <MinimizedLine view={view.action} onRestore={() => onMinimized(false)} />
  ) : (
    <ReadOnlyScope>
      <GameProvider seats={seats} dispatch={read}>
        <ActionPanel view={view.action} status={status} onMinimize={() => onMinimized(true)} />
      </GameProvider>
    </ReadOnlyScope>
  );
}

/**
 * What another seat does now, on a line over the board in the place of the stamp of the history
 * mode: "▶" and the seat, the action, the step and whom it waits for. It is there only while the
 * game waits for no decision of the viewer. A tap on it opens Watch, the turn that is played now,
 * read-only, in the place of the sheet; the line is not shown while Watch is open.
 */
export function LiveLine({ view }: { view: LiveLineView }) {
  const dispatch = useDispatch();
  return (
    <button
      type="button"
      aria-label={`Played now: ${view.text}`}
      title={view.text}
      onClick={() => dispatch({ type: "inspectTurn", turn: view.turn, watch: true })}
      className="pointer-events-auto flex max-w-full min-w-0 items-center gap-1.5 self-start rounded-md border border-line-strong bg-surface px-2 py-0.5 text-xs font-strong text-muted shadow-[0_4px_12px_#0006] hover:bg-raised touch:min-h-9"
    >
      <span aria-hidden className="shrink-0 text-accent">
        ▶
      </span>
      <span className="truncate">{view.text}</span>
    </button>
  );
}

/**
 * A turn or a phase of the Log, opened: its action as Live showed it, read-only, in the place of
 * the step sheet. It is the one sheet over the board: the viewer's open decision is not shown, its
 * bar says that it waits, and "Back" closes the turn and gives it back.
 *
 * A turn that was played is the history mode, over the board as it was then: the board is
 * hatched, a stamp over the sheet says which point it shows; the toolbar names the turn with "‹"
 * and "›" to the rows of the Log around it. The turn that is played now is Watch (`present`): the
 * live board, with no hatch and no stamp, and the toolbar of the present. Its step tabs are a view
 * of the turn; a row and a token show what was done and are no control, the inspector of a system
 * stays. "▾" minimizes it to one line, as the step sheet.
 */
export function TurnSheet({
  view,
  decision,
  fresh,
  mark,
  status,
  minimized,
  onMinimized,
  covered = false,
  present = false,
}: {
  view: InspectView;
  /** Watch: the turn that is played now, over the live board. */
  present?: boolean;
  /** The sheet of the turn is one line, so that the board is in view. */
  minimized: boolean;
  onMinimized: (minimized: boolean) => void;
  /**
   * Another sheet has the place of the turn on a phone (the inspector of a system): one sheet over
   * the board at a time. The board stays the past one.
   */
  covered?: boolean;
  /** The colour of the live workspace, for the mark of a decision that came. */
  mark: string;
  /** The viewer's open decision. Null: none. */
  decision: ActionView | null;
  /** The decision came while the screen was in the history mode. */
  fresh: boolean;
  status: string;
}) {
  const phone = usePhoneScreen();
  const dispatch = useDispatch();
  const close = () => dispatch({ type: "closeTurn" });
  const panel = (
    <ReadOnlyTurn view={view} status={status} minimized={minimized} onMinimized={onMinimized} />
  );
  // The board shows the past: it is hatched, with a dashed edge, and keeps its colours; a stamp
  // over the sheet says which point it shows, with the sign of the mode. No colour carries it.
  const past = !present && (
    <div
      aria-hidden
      data-past=""
      className="pointer-events-none absolute inset-0 z-1 border border-dashed border-line-strong bg-[repeating-linear-gradient(135deg,#ffffff0d_0_2px,transparent_2px_11px)]"
    />
  );
  const stamp = !present && (
    <p
      role="status"
      aria-label="Board shows"
      className="flex max-w-full min-w-0 items-center gap-1.5 self-start rounded-md border border-line-strong bg-surface px-2 py-0.5 text-xs font-strong text-muted shadow-[0_4px_12px_#0006]"
    >
      <Icon name="history" className="size-3.5" />
      <span className="truncate">{view.stamp}</span>
    </p>
  );
  const waiting = decision && (
    <WaitingBar
      heading={decisionHeading(decision)}
      fresh={fresh}
      mark={mark}
      onBack={close}
      edge={false}
    />
  );
  if (phone) {
    // A phone has the bar of the decision in the place of the footer, under the pane.
    return (
      <>
        {past}
        {!covered && (
          <>
            {/* The stamp is a part of what covers the board: no system is framed under it. */}
            <div
              data-covers-board=""
              className="pointer-events-none absolute inset-x-0 bottom-0 z-2 flex max-h-[60%] flex-col gap-1.5"
            >
              {stamp && <div className="flex px-2.5">{stamp}</div>}
              <div
                role="region"
                aria-label={present ? "Watch" : "Turn"}
                className="pointer-events-auto flex min-h-0 flex-1 flex-col overflow-auto border-t border-line-strong bg-surface"
              >
                {panel}
              </div>
            </div>
          </>
        )}
      </>
    );
  }
  const frame =
    "flex w-[412px] flex-col overflow-hidden rounded-lg border border-line-strong bg-surface shadow-[0_12px_30px_#0008]";
  return (
    <>
      {past}
      <div className="absolute right-3 bottom-3 z-2 flex max-h-[80%] w-[412px] flex-col gap-2">
        {stamp}
        <div
          role="region"
          aria-label={present ? "Watch" : "Turn"}
          className={`${frame} min-h-0 flex-1`}
        >
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
