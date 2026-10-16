import type { ActionView } from "../../model";
import { ActionPanel } from "./ActionPanel";

/**
 * The open step as a sheet over the bottom of the board: the whole action panel (the heading row,
 * the band of a window, the blocks or the steps of a tactical action). It has at most about 60% of
 * the board, and its content scrolls inside it, so the board stays in view above it. With no step
 * open it is the heading row, which says whom the game waits for. A phone shows the footer of the
 * action under every pane, so the sheet has none.
 */
export function StepSheet({ view, status }: { view: ActionView; status: string }) {
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
