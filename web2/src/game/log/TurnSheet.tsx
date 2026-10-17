import type { ActionView, InspectView, Intent } from "../../model";
import { PhoneScope, ReadOnlyScope, usePhoneScreen } from "../../ui";
import { ActionFooter, ActionPanel, footerOf } from "../action/ActionPanel";
import { MinimizedLine } from "../action/StepSheet";
import { GameProvider, useDispatch, useSeats } from "../context";

/**
 * A turn of the Log, opened: its action as Live showed it, read-only, in the place of the step
 * sheet. Its step tabs are a view of the turn; a row and a token show what was done and are no
 * control, the inspector of a system stays. "×" closes it. The viewer's open decision is its
 * minimized line under it, with the footer of the action on desktop (a phone has the footer under
 * the pane): a tap on the line closes the turn and gives the decision back.
 */
export function TurnSheet({
  view,
  decision,
  status,
}: {
  view: InspectView;
  /** The viewer's open decision. Null: none. */
  decision: ActionView | null;
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
  const panel = (
    <ReadOnlyScope>
      <GameProvider seats={seats} dispatch={read}>
        <ActionPanel view={view.action} status={status} onClose={close} />
      </GameProvider>
    </ReadOnlyScope>
  );
  const line = decision && <MinimizedLine view={decision} onRestore={close} />;
  if (phone) {
    return (
      <div
        role="region"
        aria-label="Turn"
        data-covers-board=""
        className="absolute inset-x-0 bottom-0 z-2 flex max-h-[60%] flex-col border-t border-line bg-surface"
      >
        <div className="flex min-h-0 flex-1 flex-col overflow-auto">{panel}</div>
        {line && <div className="border-t border-line">{line}</div>}
      </div>
    );
  }
  const frame =
    "flex w-[412px] flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-[0_12px_30px_#0008]";
  return (
    <div className="absolute right-3 bottom-3 z-2 flex max-h-[80%] flex-col gap-2">
      <div role="region" aria-label="Turn" className={`${frame} min-h-0 flex-1`}>
        <PhoneScope className="flex min-h-0 flex-1 flex-col">{panel}</PhoneScope>
      </div>
      {decision && (
        <div role="region" aria-label="Open step" className={`${frame} flex-none`}>
          <PhoneScope className="flex min-h-0 flex-1 flex-col">
            {line}
            <ActionFooter view={footerOf(decision)} />
          </PhoneScope>
        </div>
      )}
    </div>
  );
}
