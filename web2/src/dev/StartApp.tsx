import { useCallback, useEffect, useState } from "react";
import { StartScreen } from "../game/lobby/StartScreen";
import { NICKNAME_KEY, keptNickname, validNickname } from "../session/onlineKeys";
import { SINGLE_ADDRESS } from "../session/savedGame";
import {
  type OfflineSaveStore,
  type SaveRow,
  removeSave,
  saveRows,
  saveStore,
  storeWarning,
  takeLeft,
} from "../session/saves";
import { DEFAULT_NICKNAME, onlineAddress } from "./onlineStart";

/** The saved games of this browser, as the start screen lists them. */
interface Saves {
  rows: SaveRow[];
  /** Saved games of an older version, which are not read. */
  old: number;
  warning: string | null;
}

/**
 * The bare address: the choice between single player and multiplayer, and the saved games of
 * this browser. "Single player" opens the game of this browser, or the setup of a new one;
 * "Multiplayer" makes a table on the server and opens its lobby. A saved game opens at its
 * address.
 */
export function StartApp() {
  const [nickname, setNickname] = useState(
    () => localStorage.getItem(NICKNAME_KEY) ?? DEFAULT_NICKNAME,
  );
  const [store, setStore] = useState<OfflineSaveStore | null>(null);
  const [saves, setSaves] = useState<Saves | null>(null);
  const read = useCallback(async (from: OfflineSaveStore) => {
    const rows = saveRows(await from.list());
    const status = from.status();
    setSaves({ rows, old: status.unreadable, warning: storeWarning(status) });
  }, []);
  useEffect(() => {
    let stale = false;
    saveStore().then(async (opened) => {
      // A game page that was left before the store had its last save kept it in localStorage.
      await takeLeft(opened, localStorage, keptNickname(localStorage) ?? DEFAULT_NICKNAME);
      if (!stale) {
        setStore(opened);
        void read(opened);
      }
    });
    return () => {
      stale = true;
    };
  }, [read]);
  return (
    <StartScreen
      rows={[
        {
          key: "1",
          label: "Single player",
          detail: "A game in this page",
          href: `/${SINGLE_ADDRESS}`,
        },
        {
          key: "2",
          label: "Multiplayer",
          detail: "A table on the server",
          href: `/${onlineAddress()}`,
        },
      ]}
      saves={saves?.rows ?? []}
      old={saves?.old ?? 0}
      warning={saves?.warning ?? null}
      onDelete={(id) => {
        if (store) {
          void removeSave(store, localStorage, id).then(() => read(store));
        }
      }}
      onDeleteOld={() => {
        if (store) {
          void store.removeUnreadable().then(() => read(store));
        }
      }}
      nickname={nickname}
      validName={validNickname}
      onNickname={(name) => {
        setNickname(name);
        localStorage.setItem(NICKNAME_KEY, name);
      }}
    />
  );
}
