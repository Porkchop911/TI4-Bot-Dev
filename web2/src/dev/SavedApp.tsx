import { useEffect, useState } from "react";
import { type LocalGame, gameOfSave } from "../session/savedGame";
import { keptNickname } from "../session/onlineKeys";
import { saveStore, takeLeft } from "../session/saves";
import type { HostSave } from "../session/wasmTransport";
import { GameCrash } from "./GameCrash";
import { LocalApp, Notice } from "./LocalApp";
import { DEFAULT_NICKNAME } from "./onlineStart";

/** The way out of a saved game: the start screen, which lists the saved games. */
const START = { label: "Start", href: "/" };

/**
 * A saved game by its id, at `?save=<id>`: a game that has no address of a setup (an imported
 * game, a game of web/). The save says which game it is.
 */
export function SavedApp({ id }: { id: string }) {
  const [game, setGame] = useState<LocalGame | null | undefined>(undefined);
  useEffect(() => {
    let stale = false;
    saveStore()
      .then(async (store) => {
        // A page that was left before the store had its last save kept it in localStorage.
        await takeLeft(store, localStorage, keptNickname(localStorage) ?? DEFAULT_NICKNAME);
        return store.load(id);
      })
      .then(
        (record) => stale || setGame(record ? gameOfSave(record.save as HostSave) : null),
        () => stale || setGame(null),
      );
    return () => {
      stale = true;
    };
  }, [id]);
  if (game === undefined) {
    return <Notice tone="quiet">Opening the saved game…</Notice>;
  }
  if (game === null) {
    return (
      <Notice tone="error">
        This browser has no saved game of this address. The saved games are listed on the{" "}
        <a href="/" className="text-cyan underline">
          start screen
        </a>
        .
      </Notice>
    );
  }
  return (
    <GameCrash locate={() => Promise.resolve(id)}>
      <LocalApp game={game} id={id} other={START} />
    </GameCrash>
  );
}
