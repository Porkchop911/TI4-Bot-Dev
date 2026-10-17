import type { ActionView } from "../../model";
import { PhoneScope, usePhoneScreen } from "../../ui";
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
 */
export function StepSheet({
  view,
  status,
  asked,
  minimized,
  onMinimized,
}: {
  view: ActionView;
  status: string;
  /** The viewer has a decision open: a question of the game, or a draft. */
  asked: boolean;
  minimized: boolean;
  onMinimized: (minimized: boolean) => void;
}) {
  const phone = usePhoneScreen();
  const minimize = () => onMinimized(true);
  const restore = () => onMinimized(false);
  if (phone) {
    return (
      asked && (
        <div
          role="region"
          aria-label="Open step"
          className="absolute inset-x-0 bottom-0 z-2 flex max-h-[60%] flex-col overflow-auto border-t border-line bg-surface"
        >
          {minimized ? (
            <MinimizedLine view={view} onRestore={restore} />
          ) : (
            <ActionPanel view={view} status={status} footer={false} onMinimize={minimize} />
          )}
        </div>
      )
    );
  }
  const frame =
    "absolute right-3 bottom-3 z-2 flex w-[412px] flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-[0_12px_30px_#0008]";
  if (!asked) {
    const footer = footerOf(view);
    return (
      footer.actions.length > 0 && (
        <PhoneScope className={`${frame} [&>footer]:border-t-0`}>
          <ActionFooter view={footer} />
        </PhoneScope>
      )
    );
  }
  if (minimized) {
    return (
      <div role="region" aria-label="Open step" className={frame}>
        <PhoneScope className="flex min-h-0 flex-1 flex-col">
          <MinimizedLine view={view} onRestore={restore} />
          <ActionFooter view={footerOf(view)} />
        </PhoneScope>
      </div>
    );
  }
  return (
    <div role="region" aria-label="Open step" className={`${frame} max-h-[65%]`}>
      <PhoneScope className="flex min-h-0 flex-1 flex-col">
        <ActionPanel view={view} status={status} onMinimize={minimize} />
      </PhoneScope>
    </div>
  );
}
