import { useCallback, useEffect, useMemo, useState } from "react";
import { LobbyScreen } from "../game/lobby/LobbyScreen";
import type { LobbyIntent } from "../model";
import { useChosenBot } from "../session/chosenBot";
import { localTable } from "../session/localTable";
import { type ServedModel, listModels } from "../session/models";
import { PLAYER_COUNT_KEY, keptNickname, validNickname } from "../session/onlineKeys";
import { type LocalGame, SINGLE_ADDRESS, gameOfSave, singleAddress } from "../session/savedGame";
import { newSaveId, saveStore, takeLeft } from "../session/saves";
import { selectLobby } from "../session/select/lobby";
import type { Table, TableState } from "../session/table";
import { type Engine, type HostSave, loadEngine } from "../session/wasmTransport";
import { GameCrash } from "./GameCrash";
import { LocalApp, Notice, wasmUrl } from "./LocalApp";
import { DEFAULT_NICKNAME } from "./onlineStart";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The way out of single player: the start screen. */
const START = { label: "Start", href: "/" };
/** The seats of a table when the browser made none before. */
const DEFAULT_PLAYERS = 6;

/** What `?single` names of a new table: `?single&seed=3&players=4`. Each part can be left out. */
export interface SingleStart {
  /** Without it the page draws one. */
  seed?: number;
  /** The number of seats, 2 to 8. Without it: what this browser used last. */
  players?: number;
}

export function singleStart(params: URLSearchParams): SingleStart {
  const seed = Number(params.get("seed") || Number.NaN);
  const players = Number(params.get("players") || Number.NaN);
  return {
    ...(Number.isInteger(seed) && seed >= 0 && seed <= 0xffff_ffff ? { seed } : {}),
    ...(Number.isInteger(players) && players >= 2 && players <= 8 ? { players } : {}),
  };
}

/** The setup of a table in the page: the lobby screen over `localTable`. */
function SingleSetup({
  start,
  onStart,
}: {
  start: SingleStart;
  onStart: (game: LocalGame) => void;
}) {
  const [engine, setEngine] = useState<Engine | null>(null);
  /** The models that the server offers; none with no server. Asked once, with the engine. */
  const [models, setModels] = useState<ServedModel[] | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [table, setTable] = useState<Table | null>(null);
  const [state, setState] = useState<TableState | null>(null);
  const [toast, setToast] = useState<{ id: number; text: string } | null>(null);
  const [inspected, setInspected] = useState<string | null>(null);
  const [bot, chooseBot] = useChosenBot();

  useEffect(() => {
    let stale = false;
    Promise.all([loadEngine(wasmUrl), listModels()])
      .then(([loaded, served]) => {
        if (!stale) {
          setModels(served ?? []);
          setEngine(loaded);
        }
      })
      .catch((error: unknown) => stale || setFailure(message(error)));
    return () => {
      stale = true;
    };
  }, []);

  const say = useCallback((text: string) => setToast({ id: performance.now(), text }), []);
  // Made in an effect, as the game of `LocalApp`: each mount gets a table of its own.
  useEffect(() => {
    if (!engine) {
      return;
    }
    const kept = Number(localStorage.getItem(PLAYER_COUNT_KEY) || Number.NaN);
    const made = localTable(engine.module, {
      nickname: keptNickname(localStorage) ?? DEFAULT_NICKNAME,
      players:
        start.players ??
        (Number.isInteger(kept) && kept >= 2 && kept <= 8 ? kept : DEFAULT_PLAYERS),
      seed: start.seed ?? crypto.getRandomValues(new Uint32Array(1))[0],
      onStart,
      models: models ?? [],
    });
    const stop = made.subscribe((event) => {
      switch (event.kind) {
        case "table":
          return setState(event);
        case "rejected":
          return say(event.reason);
        case "error":
          return setFailure(event.message);
        default:
          return;
      }
    });
    setTable(made);
    return () => {
      stop();
      made.close();
      setTable(null);
    };
  }, [engine, models, start, onStart, say]);

  const view = useMemo(
    () =>
      state &&
      selectLobby(state, { mine: [], inspected, offline: false, bot }, engine?.catalog ?? null),
    [state, inspected, engine, bot],
  );

  // The engine did not load.
  if (failure) {
    return <Notice tone="error">Single player unavailable. {failure}</Notice>;
  }
  if (!view) {
    return <Notice tone="quiet">Setting up the table…</Notice>;
  }

  const dispatch = (intent: LobbyIntent) => {
    switch (intent.type) {
      case "inspectSystem":
        return setInspected(intent.system);
      case "closeInspector":
        return setInspected(null);
      case "chooseBot":
        return chooseBot(intent.kind);
      case "addBot":
        return table?.addBots(intent.kind);
      case "fillBots":
        return table?.addBots(intent.kind, true);
      case "remove":
        return table?.removeSeat(intent.player);
      case "shuffleSeats":
        return table?.shuffleSeats();
      case "set":
        if (intent.change.players !== undefined) {
          // The next table of this browser has as many seats.
          localStorage.setItem(PLAYER_COUNT_KEY, String(intent.change.players));
        }
        return table?.setSettings(intent.change);
      case "faction":
        return table?.setFaction(intent.player, intent.faction);
      case "start":
        return table?.start();
      // A table in the page has one person: nobody joins it, and its player has no other tab.
      default:
        return;
    }
  };

  return (
    <div className="flex h-dvh flex-col">
      <div className="relative grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)]">
        <LobbyScreen
          view={view}
          dispatch={dispatch}
          nickname=""
          onNickname={() => {}}
          validName={validNickname}
          toast={toast}
          other={START}
        />
      </div>
    </div>
  );
}

/** What single player shows: the setup of a table, or a game by its id. */
type Shown =
  | { kind: "setup" }
  | { kind: "loading"; id: string }
  | { kind: "missing" }
  | { kind: "game"; id: string; game: LocalGame; fresh: boolean };

/**
 * Single player. `?single` is the setup of a new table; the game that it starts is kept under an
 * id of its own, at `?single=<id>`, so a browser keeps as many games of single player as it
 * plays. "New game" of the Dev sheet opens the setup, and the game stays in the saved games.
 */
export function SingleApp({ id, start }: { id: string | null; start: SingleStart }) {
  const [shown, setShown] = useState<Shown>(() =>
    id === null ? { kind: "setup" } : { kind: "loading", id },
  );
  const loading = shown.kind === "loading" ? shown.id : null;
  useEffect(() => {
    if (loading === null) {
      return;
    }
    let stale = false;
    saveStore()
      .then(async (store) => {
        // A page that was left before the store had its last save kept it in localStorage.
        await takeLeft(store, localStorage, keptNickname(localStorage) ?? DEFAULT_NICKNAME);
        return store.load(loading);
      })
      .then(
        (record) =>
          stale ||
          setShown(
            record
              ? {
                  kind: "game",
                  id: loading,
                  game: gameOfSave(record.save as HostSave),
                  fresh: false,
                }
              : { kind: "missing" },
          ),
        () => stale || setShown({ kind: "missing" }),
      );
    return () => {
      stale = true;
    };
  }, [loading]);
  // The game is kept from its first question on (`LocalApp`): a reload of its address is then
  // this game.
  const started = useCallback((game: LocalGame) => {
    const made = newSaveId();
    history.replaceState(null, "", `/${singleAddress(made)}`);
    setShown({ kind: "game", id: made, game, fresh: true });
  }, []);
  // A page load: the setup of a new table, on a seed of its own.
  const again = useCallback(() => location.assign(`/${SINGLE_ADDRESS}`), []);
  switch (shown.kind) {
    case "setup":
      return <SingleSetup start={start} onStart={started} />;
    case "loading":
      return <Notice tone="quiet">Opening the saved game…</Notice>;
    case "missing":
      return (
        <Notice tone="error">
          This browser has no saved game of this address. The saved games are listed on the{" "}
          <a href="/" className="text-cyan underline">
            start screen
          </a>
          .
        </Notice>
      );
    default:
      return (
        <GameCrash locate={() => Promise.resolve(shown.id)}>
          <LocalApp
            game={shown.game}
            id={shown.id}
            fresh={shown.fresh}
            onNewGame={again}
            other={START}
          />
        </GameCrash>
      );
  }
}
