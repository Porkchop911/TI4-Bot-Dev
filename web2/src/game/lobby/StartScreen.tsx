import { useEffect, useState } from "react";
import { Button, Dialog, Kbd, usePhoneScreen } from "../../ui";
import { keyIsFree } from "../shell/shortcuts";
import { NICKNAME_MAX, type ValidName } from "./LobbyScreen";

/** One way into a game. The row is the control; a choice is a page load. */
export interface StartRow {
  key: string;
  label: string;
  /** What the row opens, in a few words. */
  detail: string;
  href: string;
}

/** A saved game of this browser. The row is the control that opens it. */
export interface SavedRow {
  id: string;
  href: string;
  /** Which kind of game. */
  title: string;
  /** Where the game is, and who plays it here. */
  detail: string;
  /** When it was saved last. */
  updated: string;
}

/** What a dialog asks to delete: one saved game, or the saved games of an older version. */
type Deleting = { row: SavedRow } | { old: number };

/** The first screen: the choice between the kinds of game, and the saved games. */
export function StartScreen({
  rows,
  saves = [],
  old = 0,
  warning = null,
  onDelete,
  onDeleteOld,
  nickname,
  onNickname,
  validName,
}: {
  rows: StartRow[];
  /** The saved games of this browser, newest first. */
  saves?: SavedRow[];
  /** How many saved games are of an older version: they are not read. */
  old?: number;
  /** Why the saved games are kept in memory only. */
  warning?: string | null;
  onDelete?: (id: string) => void;
  onDeleteOld?: () => void;
  /** The name of the player at a table. It is kept for the next visit. */
  nickname: string;
  onNickname: (name: string) => void;
  /** A name that the server refuses is marked: a table is made with the default name. */
  validName: ValidName;
}) {
  const phone = usePhoneScreen();
  const [deleting, setDeleting] = useState<Deleting | null>(null);
  const confirm = () => {
    if (deleting && "row" in deleting) {
      onDelete?.(deleting.row.id);
    } else if (deleting) {
      onDeleteOld?.();
    }
    setDeleting(null);
  };
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const row = rows.find((item) => item.key === event.key);
      if (row && keyIsFree(event, false)) {
        event.preventDefault();
        location.assign(row.href);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [rows]);
  return (
    <main className="flex h-dvh flex-col items-center justify-center gap-5 bg-board px-4 pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <h1 className="text-xl font-semibold">Twilight Imperium</h1>
      <nav aria-label="Game" className="w-full max-w-[460px] rounded border border-line bg-surface">
        {rows.map((row) => (
          <a
            key={row.key}
            href={row.href}
            aria-keyshortcuts={row.key}
            className="flex min-h-14 items-center gap-3 px-4 py-2 hover:bg-cyan/[.06] [a+&]:border-t [a+&]:border-line/50"
          >
            {!phone && <Kbd>{row.key}</Kbd>}
            <strong className="text-base font-semibold">{row.label}</strong>
            <small className="ml-auto text-right text-xs text-muted">{row.detail}</small>
          </a>
        ))}
      </nav>
      <label className="flex w-full max-w-[460px] items-center gap-3 px-4 text-sm text-muted">
        Name
        <input
          value={nickname}
          maxLength={NICKNAME_MAX}
          aria-invalid={nickname.trim() !== "" && !validName(nickname.trim())}
          onChange={(event) => onNickname(event.target.value)}
          className="min-h-9 min-w-0 flex-1 rounded border border-line bg-well px-2.5 text-base text-text aria-[invalid=true]:border-red touch:min-h-11"
        />
      </label>
      {(saves.length > 0 || old > 0 || warning) && (
        <section
          aria-label="Saved games"
          className="flex max-h-[50dvh] w-full max-w-[720px] flex-col rounded border border-line bg-surface"
        >
          <h2 className="px-4 pt-2 pb-1 text-sm text-muted">Saved games</h2>
          {warning && (
            <p role="alert" className="px-4 pb-1.5 text-sm text-orange">
              ⚠ {warning}
            </p>
          )}
          <div className="min-h-0 overflow-y-auto">
            {saves.map((row) => (
              <div
                key={row.id}
                className="row flex min-h-10 items-center gap-2 pr-2.5 [.row+&]:border-t [.row+&]:border-line/50"
              >
                <a
                  href={row.href}
                  className="flex min-h-10 min-w-0 flex-1 items-center gap-3 px-4 hover:bg-cyan/[.06]"
                >
                  <strong className="shrink-0 text-base font-semibold">{row.title}</strong>
                  <span className="min-w-0 truncate text-sm text-muted" title={row.detail}>
                    {row.detail}
                  </span>
                  <small className="ml-auto shrink-0 text-xs text-muted">{row.updated}</small>
                </a>
                <Button
                  tone="quiet"
                  size="sm"
                  aria-label={`Delete ${row.title}`}
                  onClick={() => setDeleting({ row })}
                >
                  Delete
                </Button>
              </div>
            ))}
            {old > 0 && (
              <div className="row flex min-h-10 items-center gap-2 pr-2.5 pl-4 [.row+&]:border-t [.row+&]:border-line/50">
                <span className="flex-1 text-sm text-muted">
                  {old === 1 ? "1 saved game is" : `${old} saved games are`} of an older version.
                  They cannot be played again.
                </span>
                <Button
                  tone="quiet"
                  size="sm"
                  aria-label="Delete the saved games of an older version"
                  onClick={() => setDeleting({ old })}
                >
                  Delete
                </Button>
              </div>
            )}
          </div>
        </section>
      )}
      <Dialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        title={
          deleting && "old" in deleting
            ? deleting.old === 1
              ? "Delete the saved game of an older version?"
              : `Delete the ${deleting.old} saved games of an older version?`
            : "Delete the saved game?"
        }
        description={
          deleting && "row" in deleting
            ? `${deleting.row.title} · ${deleting.row.detail}`
            : undefined
        }
        actions={
          <>
            <Button tone="quiet" onClick={() => setDeleting(null)}>
              Keep
            </Button>
            <Button tone="primary" onClick={confirm}>
              Delete
            </Button>
          </>
        }
      />
    </main>
  );
}
