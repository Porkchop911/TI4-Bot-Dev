import { useLayoutEffect, useRef } from "react";
import type { LogView } from "../../model";
import { LogHistory } from "./LogHistory";

/** How near the top the Log counts as at its top, in pixels: there it shows what is new. */
const AT_TOP = 24;

/**
 * The log: what the others are doing and what they did, so that the viewer can decide. It is
 * one row for each turn since the viewer's last turn and each phase in between, the turn that is
 * played now on top. Desktop has it under the player table, where it fills the column and scrolls
 * inside itself; a phone has it as the pane Log, which scrolls the same way. The newest is on top:
 * the Log starts there, and shows what is new while it is at its top; else what is new comes in
 * above and the rows in view stay where they are. A tactical, strategic or component turn opens
 * its action over the board, read-only.
 */
export function LogPanel({
  log,
  open = null,
}: {
  log: LogView;
  /** The turn that is open over the board. */
  open?: string | null;
}) {
  const scroller = useRef<HTMLElement>(null);
  const atTop = useRef(true);
  const height = useRef(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: what is new is the trigger
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) {
      return;
    }
    // A row that came in above pushes the rows in view down by its height: they stay in view.
    if (atTop.current) {
      element.scrollTop = 0;
    } else {
      element.scrollTop += element.scrollHeight - height.current;
    }
    height.current = element.scrollHeight;
  }, [log]);
  return (
    <section
      ref={scroller}
      aria-label="Log"
      className="min-h-0 flex-1 overflow-y-auto py-2.5 [overflow-anchor:none] phone:bg-board"
      onScroll={(event) => {
        atTop.current = event.currentTarget.scrollTop <= AT_TOP;
      }}
    >
      <LogHistory view={log} open={open} />
    </section>
  );
}
