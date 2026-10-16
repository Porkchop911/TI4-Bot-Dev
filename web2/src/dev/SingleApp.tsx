import { useCallback, useEffect, useMemo, useState } from "react";
import { LobbyScreen } from "../game/lobby/LobbyScreen";
import type { LobbyIntent } from "../model";
import { localTable } from "../session/localTable";
import { NICKNAME_KEY, PLAYER_COUNT_KEY } from "../session/onlineKeys";
import {
  type LocalGame,
  SINGLE_KEY,
  clearSavedGame,
  parseSavedGame,
  storeSavedGame,
} from "../session/savedGame";
import { selectLobby } from "../session/select/lobby";
import type { Table, TableState } from "../session/table";
import { type Engine, loadEngine } from "../session/wasmTransport";
import { DemoApp } from "./DemoApp";
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

/**
 * The game of single player as it was kept, or null. A save of an older format is still the game
 * of this browser: its page says that it is not read, with "New game".
 */
function keptGame(): LocalGame | null {
  const text = localStorage.getItem(SINGLE_KEY);
  if (text === null) {
    return null;
  }
  try {
    const { seed, players, humans, table, waitMs } = parseSavedGame(text);
    return table ? { seed, players, humans, table, ...(waitMs ? { waitMs } : {}) } : null;
  } catch {
    try {
      const old: Partial<LocalGame> | null = JSON.parse(text);
      return old?.table ? (old as LocalGame) : null;
    } catch {
      return null;
    }
  }
}

/** The setup of a table in the page: the lobby screen over `localTable`. */
function SingleSetup({
  start,
  onStart,
}: {
  start: SingleStart;
  onStart: (game: LocalGame, engine: Engine) => void;
}) {
  const [engine, setEngine] = useState<Engine | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [table, setTable] = useState<Table | null>(null);
  const [state, setState] = useState<TableState | null>(null);
  const [toast, setToast] = useState<{ id: number; text: string } | null>(null);
  const [inspected, setInspected] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    loadEngine(wasmUrl)
      .then((loaded) => stale || setEngine(loaded))
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
      nickname: localStorage.getItem(NICKNAME_KEY)?.trim() || DEFAULT_NICKNAME,
      players:
        start.players ??
        (Number.isInteger(kept) && kept >= 2 && kept <= 8 ? kept : DEFAULT_PLAYERS),
      seed: start.seed ?? crypto.getRandomValues(new Uint32Array(1))[0],
      onStart: (game) => onStart(game, engine),
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
  }, [engine, start, onStart, say]);

  const view = useMemo(
    () =>
      state && selectLobby(state, { mine: [], inspected, offline: false }, engine?.catalog ?? null),
    [state, inspected, engine],
  );

  // The engine did not load: the demo is shown with the reason, as for a local game.
  if (failure) {
    return <DemoApp notice={`Single player unavailable. ${failure}`} />;
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
      case "addBot":
        return table?.addBots(intent.kind);
      case "fillBots":
        return table?.addBots(intent.kind, true);
      case "remove":
        return table?.removeSeat(intent.player);
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
          toast={toast}
          other={START}
        />
      </div>
    </div>
  );
}

/**
 * Single player, at `?single`: the game that this browser kept, or with none the setup of a
 * table in the page. The address names the save, not the setup: a reload of a started game is
 * that game. "New game" of its Dev sheet drops the game and opens the setup.
 */
export function SingleApp({ start }: { start: SingleStart }) {
  const [game, setGame] = useState(keptGame);
  const started = useCallback((made: LocalGame, engine: Engine) => {
    // Kept before the first answer: a reload is then this game, not another setup.
    storeSavedGame(localStorage, { format: 3, engine: engine.hash, ...made, decisions: [] });
    setGame(made);
  }, []);
  const again = useCallback(() => {
    if (game) {
      clearSavedGame(localStorage, game);
    }
    setGame(null);
  }, [game]);
  return game ? (
    <LocalApp game={game} onNewGame={again} other={START} />
  ) : (
    <SingleSetup start={start} onStart={started} />
  );
}
