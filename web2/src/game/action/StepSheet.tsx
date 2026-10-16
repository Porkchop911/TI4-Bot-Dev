import type { ActionView } from "../../model";
import { PhoneScope, usePhoneScreen } from "../../ui";
import { ActionPanel } from "./ActionPanel";

/**
 * The open step as a sheet over the board: the whole action panel (the heading row, the band of a
 * window, the blocks or the steps of a tactical action). The board stays in view: the sheet has a
 * part of it, and its content scrolls inside it. With no step open it is the heading row, which
 * says whom the game waits for.
 *
 * A phone has it over the bottom of the board, and shows the footer of the action under every
 * pane. Desktop has it at the bottom right of the board in the phone layout, at the width of a
 * phone (`PhoneScope`), with the footer inside it.
 */
export function StepSheet({ view, status }: { view: ActionView; status: string }) {
  const phone = usePhoneScreen();
  if (phone) {
    return (
      <div
        role="region"
        aria-label="Open step"
        className="absolute inset-x-0 bottom-0 z-2 flex max-h-[60%] flex-col overflow-auto border-t border-line bg-surface"
      >
        <ActionPanel view={view} status={status} footer={false} />
      </div>
    );
  }
  return (
    <div
      role="region"
      aria-label="Open step"
      className="absolute right-3 bottom-3 z-2 flex max-h-[65%] w-[412px] flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-[0_12px_30px_#0008]"
    >
      <PhoneScope className="flex min-h-0 flex-1 flex-col">
        <ActionPanel view={view} status={status} />
      </PhoneScope>
    </div>
  );
}
