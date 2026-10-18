import type { ReactNode } from "react";
import { cx } from "./cx";

/**
 * The heading of a group that opens and closes with a click: "▸" closed, "▾" open. The groups of
 * a deck in the sheets, and the parts of the current action in the Log.
 */
export function GroupHeading({
  open,
  onToggle,
  name,
  count,
  className,
}: {
  open: boolean;
  onToggle: () => void;
  name: ReactNode;
  /** What the group holds, muted after the name: how many it has, or a state. */
  count?: ReactNode;
  /** The space around it; a group of a sheet has room above it. */
  className?: string;
}) {
  return (
    <h4>
      <button
        type="button"
        className={cx(
          "flex w-full min-w-0 items-baseline gap-2 border-b border-line text-left font-strong",
          className ?? "mt-3 py-1.5",
        )}
        aria-expanded={open}
        onClick={onToggle}
      >
        <span aria-hidden="true">{open ? "▾" : "▸"}</span>
        {name}
        {count != null && (
          <span className="min-w-0 truncate text-xs font-normal text-muted tabular-nums">
            {count}
          </span>
        )}
      </button>
    </h4>
  );
}
