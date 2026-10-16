import type { CSSProperties, ReactNode } from "react";
import { cx, usePhoneScreen } from "../../ui";

/** One part of the shell that a phone shows alone. */
export interface ShellPane {
  id: string;
  label: string;
  content: ReactNode;
  /** The pane is a block that scrolls down, not a part that fills the pane itself. */
  scrolls?: boolean;
}

/**
 * The frame of a screen of the game: the header, then the board and the right column, which has
 * one fixed width. A portrait phone shows the same parts one at a time, with a tab bar at the
 * bottom and the footer under every pane, so the main button is always in reach. The game and
 * the lobby of a table use it.
 */
export function ShellFrame({
  accent,
  header,
  board,
  column,
  panes,
  pane,
  onPane,
  marked,
  footer,
  sheet,
  children,
}: {
  /** The colour of the state of the screen: the main button and the marks have it. */
  accent: string;
  header: ReactNode;
  /** Desktop: the left of the screen. */
  board: ReactNode;
  /** Desktop: the right column. */
  column: ReactNode;
  /** Phone: the panes, in the order of the tab bar. */
  panes: ShellPane[];
  pane: string;
  onPane: (id: string) => void;
  /** Phone: the pane where a choice is open, when it is not in view. */
  marked?: string | null;
  /** Phone: under every pane. */
  footer: ReactNode;
  /** Phone: a sheet that covers the pane. It has the room of the footer and of the tab bar. */
  sheet?: ReactNode;
  /** Dialogs and messages. */
  children?: ReactNode;
}) {
  const phone = usePhoneScreen();
  return (
    <main className="flex min-h-0 flex-col" style={{ "--accent": accent } as CSSProperties}>
      {header}
      {phone ? (
        <>
          <div className="relative min-h-0 flex-1">
            {/* Every pane stays in the page, so the board keeps its camera and its size. */}
            {panes.map((item) => (
              <div
                key={item.id}
                className={cx(
                  "absolute inset-0 grid grid-cols-[minmax(0,1fr)] grid-rows-[minmax(0,1fr)]",
                  item.scrolls && "block! overflow-auto bg-board",
                  pane !== item.id && "invisible",
                )}
                inert={pane !== item.id}
              >
                {item.content}
              </div>
            ))}
            {sheet}
          </div>
          {!sheet && footer}
          {!sheet && (
            <nav
              aria-label="Panes"
              className="flex border-t border-line bg-well pb-(--inset-bottom,env(safe-area-inset-bottom))"
            >
              {panes.map(({ id, label }) => (
                <button
                  key={id}
                  type="button"
                  aria-pressed={pane === id}
                  onClick={() => onPane(id)}
                  className="min-h-12 flex-1 border-t-2 border-transparent text-sm font-strong text-muted aria-pressed:border-accent aria-pressed:text-accent"
                >
                  {label}
                  {marked === id && pane !== id && (
                    <>
                      <span
                        aria-hidden
                        className="ml-1.5 inline-block size-2 rounded-full bg-accent align-middle shadow-[0_0_6px_var(--color-accent)] animate-blink motion-reduce:animate-none"
                      />
                      <span className="sr-only">, choice open</span>
                    </>
                  )}
                </button>
              ))}
            </nav>
          )}
        </>
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_1040px]">
          {board}
          <div className="flex min-h-0 min-w-0 flex-col border-l border-line">{column}</div>
        </div>
      )}
      {children}
    </main>
  );
}
