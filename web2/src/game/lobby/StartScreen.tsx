import { useEffect } from "react";
import { Kbd, usePhone } from "../../ui";
import { keyIsFree } from "../shell/shortcuts";
import { NICKNAME_MAX } from "./LobbyScreen";

/** One way into a game. The row is the control; a choice is a page load. */
export interface StartRow {
  key: string;
  label: string;
  /** What the row opens, in a few words. */
  detail: string;
  href: string;
}

/** The first screen: the choice between the kinds of game. */
export function StartScreen({
  rows,
  nickname,
  onNickname,
}: {
  rows: StartRow[];
  /** The name of the player at a table. It is kept for the next visit. */
  nickname: string;
  onNickname: (name: string) => void;
}) {
  const phone = usePhone();
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
          onChange={(event) => onNickname(event.target.value)}
          className="min-h-9 min-w-0 flex-1 rounded border border-line bg-well px-2.5 text-base text-text touch:min-h-11"
        />
      </label>
    </main>
  );
}
