import { useState } from "react";
import { StartScreen } from "../game/lobby/StartScreen";
import { NICKNAME_KEY } from "../session/onlineKeys";
import { SINGLE_ADDRESS, SINGLE_KEY, saveKey } from "../session/savedGame";
import { DEFAULT_GAME, SINGLE_PLAYER } from "./defaultGame";
import { DEFAULT_NICKNAME, onlineAddress } from "./onlineStart";

/**
 * Where "Single player" goes: the game that this browser kept, or with none the setup of a
 * table in the page. A browser that only has the game of before the setup (seed 42, eight seats)
 * opens that game, so nobody loses a saved game.
 */
export const singlePlayer = (storage: Pick<Storage, "getItem">): string =>
  storage.getItem(SINGLE_KEY) === null && storage.getItem(saveKey(DEFAULT_GAME)) !== null
    ? SINGLE_PLAYER
    : `/${SINGLE_ADDRESS}`;

/**
 * The bare address: the choice between single player and multiplayer. "Single player" opens the
 * game of this browser, or the setup of a new one; "Multiplayer" makes a table on the server and
 * opens its lobby.
 */
export function StartApp() {
  const [nickname, setNickname] = useState(
    () => localStorage.getItem(NICKNAME_KEY) ?? DEFAULT_NICKNAME,
  );
  return (
    <StartScreen
      rows={[
        {
          key: "1",
          label: "Single player",
          detail: "A game in this page",
          href: singlePlayer(localStorage),
        },
        {
          key: "2",
          label: "Multiplayer",
          detail: "A table on the server",
          href: `/${onlineAddress()}`,
        },
      ]}
      nickname={nickname}
      onNickname={(name) => {
        setNickname(name);
        localStorage.setItem(NICKNAME_KEY, name);
      }}
    />
  );
}
