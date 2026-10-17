import { useLayoutEffect, useRef } from "react";
import type { LogView } from "../../model";
import { LogHistory } from "./LogHistory";

/** How near the end the Log counts as at its end, in pixels: it follows what is new. */
const AT_END = 24;

/**
 * The log: what the others are doing and what they did, so that the viewer can decide. It is
 * one row for each turn since the viewer's last turn and each phase in between, the turn that is
 * played now last. Desktop has it under the player table, where it fills the column and scrolls
 * inside itself; a phone has it as the pane Log, which scrolls the same way. The newest is at the
 * end: the Log starts there, and follows what is new while it is at its end; else it stays where
 * the viewer scrolled.
 */
export function LogPanel({ log }: { log: LogView }) {
  const scroller = useRef<HTMLElement>(null);
  const atEnd = useRef(true);
  // biome-ignore lint/correctness/useExhaustiveDependencies: what is new is the trigger
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && atEnd.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [log]);
  return (
    <section
      ref={scroller}
      aria-label="Log"
      className="min-h-0 flex-1 overflow-y-auto py-2.5 phone:bg-board"
      onScroll={(event) => {
        const element = event.currentTarget;
        atEnd.current = element.scrollHeight - element.scrollTop - element.clientHeight <= AT_END;
      }}
    >
      <LogHistory view={log} />
    </section>
  );
}
