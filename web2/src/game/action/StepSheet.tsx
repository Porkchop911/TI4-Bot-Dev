import type { ReactNode } from "react";
import type { ActionView } from "../../model";
import { cx, PhoneScope, usePhoneScreen } from "../../ui";
import { ActionFooter, ActionPanel, footerOf } from "./ActionPanel";

/**
 * The minimized step sheet: one line with the heading and its context, cut with "…". The whole
 * line restores the sheet ("▴").
 */
export function MinimizedLine({ view, onRestore }: { view: ActionView; onRestore: () => void }) {
  const heading = view.kind === "tactical" ? view.task.title : view.heading;
  return (
    <button
      type="button"
      aria-label="Restore"
      aria-expanded={false}
      title={view.context ? `${heading} · ${view.context}` : heading}
      onClick={onRestore}
      className="flex w-full min-w-0 items-baseline gap-x-2.5 px-3 py-2 text-left hover:bg-white/5 touch:min-h-12 touch:items-center"
    >
      <span aria-hidden="true" className="shrink-0 text-muted">
        ▴
      </span>
      <strong className="min-w-0 shrink truncate text-lg font-semibold tracking-[-.2px]">
        {heading}
      </strong>
      {view.context && (
        <span className="min-w-0 shrink-[2] truncate text-sm text-muted">{view.context}</span>
      )}
    </button>
  );
}

/**
 * The open decision of the viewer as a sheet over the board: the whole action panel (the heading
 * row, the band of a window, the blocks or the steps of a tactical action). The board stays in
 * view: the sheet has a part of it, and its content scrolls inside it. With no decision of the
 * viewer open (another seat's turn, waiting, a turn that ended) there is no sheet and the board is
 * free: what happens is the Log's, and whom the game waits for is the status of the toolbar.
 *
 * A phone has it over the bottom of the board, and shows the footer of the action under every
 * pane. Desktop has it at the bottom right of the board in the phone layout, at the width of a
 * phone (`PhoneScope`), with the footer inside it; with no decision open the footer alone is there
 * while it has a button (an undo while the viewer waits), as a phone has it under every pane.
 *
 * "▾" before the heading minimizes the sheet to one line, so that the board is in view; the line
 * restores it. The main button stays in reach: under the line on desktop, under the pane on a
 * phone. The shell keeps the state until a new question of the viewer.
 *
 * Over it is the line of what another seat does now (`lead`), and Watch (`watch`): the turn that
 * is played now, read-only, above the decision, which is then its line (its footer stays); a tap
 * on that line leaves Watch.
 */
export function StepSheet({
  view,
  status,
  asked,
  minimized,
  onMinimized,
  lead = null,
  watch = null,
  onLeaveWatch,
}: {
  view: ActionView;
  status: string;
  /** The viewer has a decision open: a question of the game, or a draft. */
  asked: boolean;
  minimized: boolean;
  onMinimized: (minimized: boolean) => void;
  /** The line over the sheet: what another seat does now. */
  lead?: ReactNode;
  /** Watch: the turn that is played now, read-only, in the place of the sheet. */
  watch?: ReactNode;
  onLeaveWatch?: () => void;
}) {
  const phone = usePhoneScreen();
  const minimize = () => onMinimized(true);
  const restore = () => (watch ? onLeaveWatch?.() : onMinimized(false));
  const line = asked && (minimized || !!watch);
  const watching = watch && (
    <div
      role="region"
      aria-label="Watch"
      className={cx(
        "pointer-events-auto flex min-h-0 flex-col overflow-auto bg-surface",
        phone ? "flex-1 border-t border-line-strong" : "",
      )}
    >
      {watch}
    </div>
  );
  if (phone) {
    const decision = asked && (
      <div
        role="region"
        aria-label="Open step"
        className={cx(
          "pointer-events-auto flex flex-col bg-surface",
          watch
            ? "flex-none border-t border-line"
            : "min-h-0 flex-1 overflow-auto border-t border-line",
        )}
      >
        {line ? (
          <MinimizedLine view={view} onRestore={restore} />
        ) : (
          <ActionPanel view={view} status={status} footer={false} onMinimize={minimize} />
        )}
      </div>
    );
    return (
      (lead || watching || decision) && (
        <div
          data-covers-board=""
          className="pointer-events-none absolute inset-x-0 bottom-0 z-2 flex max-h-[60%] flex-col gap-1.5"
        >
          {lead && <div className="flex px-2.5">{lead}</div>}
          {(watching || decision) && (
            <div className="flex min-h-0 flex-1 flex-col">
              {watching}
              {decision}
            </div>
          )}
        </div>
      )
    );
  }
  const frame =
    "pointer-events-auto flex w-[412px] flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-[0_12px_30px_#0008]";
  const footer = footerOf(view);
  const decision = asked ? (
    line ? (
      <div role="region" aria-label="Open step" className={`${frame} flex-none`}>
        <PhoneScope className="flex min-h-0 flex-1 flex-col">
          <MinimizedLine view={view} onRestore={restore} />
          <ActionFooter view={footer} />
        </PhoneScope>
      </div>
    ) : (
      <div role="region" aria-label="Open step" className={`${frame} min-h-0 flex-1`}>
        <PhoneScope className="flex min-h-0 flex-1 flex-col">
          <ActionPanel view={view} status={status} onMinimize={minimize} />
        </PhoneScope>
      </div>
    )
  ) : (
    footer.actions.length > 0 && (
      <PhoneScope className={`${frame} flex-none [&>footer]:border-t-0`}>
        <ActionFooter view={footer} />
      </PhoneScope>
    )
  );
  return (
    (lead || watching || decision) && (
      <div
        className={cx(
          "pointer-events-none absolute right-3 bottom-3 z-2 flex w-[412px] flex-col items-end gap-2",
          watch ? "max-h-[80%]" : "max-h-[65%]",
        )}
      >
        {lead && <div className="flex w-full">{lead}</div>}
        {watching && (
          <div className={`${frame} min-h-0 flex-1`}>
            <PhoneScope className="flex min-h-0 flex-1 flex-col">{watching}</PhoneScope>
          </div>
        )}
        {decision}
      </div>
    )
  );
}
