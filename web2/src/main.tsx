import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { DemoApp } from "./dev/DemoApp";
import { Gallery } from "./dev/Gallery";
import { LocalApp, localGame } from "./dev/LocalApp";
import { OnlineApp } from "./dev/OnlineApp";
import { SavedApp } from "./dev/SavedApp";
import { SingleApp, singleStart } from "./dev/SingleApp";
import { StartApp } from "./dev/StartApp";
import { onlineStart } from "./dev/onlineStart";
import { dropOldSaves } from "./session/savedGame";
import "./styles/theme.css";

const params = new URLSearchParams(location.search);
// `?online` makes a table on the server (with `players`, `seed`, `name` and `map` when the
// address has them), and `?online=<game_id>&seat=<player_id>` is that table and its game.
// `?single` is single player: the setup of a table in the page, or the game that was started
// from it. `?local=…` and `?scenario=…` are a game in the page on a fixed setup, `?example=…` the
// demo, `?gallery` the gallery. The bare address is the choice between single player and
// multiplayer, with the saved games. `?save=<id>` is a saved game that has no such address (an
// imported game).
const online = params.has("online");
const local = online ? null : localGame(params);
const saved = online ? null : params.get("save");
// The saves of before the save store are not read.
dropOldSaves(localStorage);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {online ? (
      <OnlineApp
        gameId={params.get("online") || null}
        seat={params.get("seat") || null}
        start={onlineStart(params)}
      />
    ) : params.has("single") ? (
      <SingleApp start={singleStart(params)} />
    ) : saved ? (
      <SavedApp id={saved} />
    ) : local ? (
      <LocalApp game={local} />
    ) : params.has("gallery") ? (
      <Gallery />
    ) : params.has("example") ? (
      <DemoApp />
    ) : (
      <StartApp />
    )}
  </StrictMode>,
);
