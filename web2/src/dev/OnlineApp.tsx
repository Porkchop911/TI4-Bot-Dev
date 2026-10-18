import { useCallback, useEffect, useMemo, useState } from "react";
import { LobbyScreen } from "../game/lobby/LobbyScreen";
import { GameShell } from "../game/shell/GameShell";
import type { LobbyIntent } from "../model";
import {
  NICKNAME_KEY,
  PLAYER_COUNT_KEY,
  dropPlayer,
  keepPlayer,
  keptNickname,
  legacySession,
  legacyStaged,
  moveLegacy,
  playersOf,
  validNickname,
} from "../session/onlineKeys";
import { useChosenBot } from "../session/chosenBot";
import { hostChange, nextNickname, selectLobby } from "../session/select/lobby";
import {
  type Seat,
  type Table,
  type TableState,
  createTable,
  playerOf,
  serverTable,
} from "../session/table";
import type { Transport } from "../session/transport";
import { useLiveSession } from "../session/useLiveSession";
import { type Engine, loadEngine } from "../session/wasmTransport";
import { createWsTransport } from "../session/wsTransport";
import { Button } from "../ui";
import { GameTable } from "./GameTable";
import { Notice, wasmUrl } from "./LocalApp";
import { DEFAULT_NICKNAME, type OnlineStart, tableAddress, tableRequest } from "./onlineStart";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The way out of a table: the start screen. */
const START = { label: "Start", href: "/" };

/**
 * A visitor takes the seat of a person who is away, under the name of the seat. The browser
 * keeps the player, and the page is loaded as that player.
 */
function rejoin(table: Table, gameId: string, player: string, name: string) {
  void table.takeOver(player, name).then((taken) => {
    if (taken) {
      keepPlayer(localStorage, gameId, taken.playerId, taken.playerSession);
      location.replace(tableAddress(gameId, taken.playerId));
    }
  });
}

/** The table that `?online` asked for. One for the page: React may mount twice (StrictMode). */
let created: ReturnType<typeof createTable> | null = null;

/** The page without a table or a game: what it waits for, or why it stopped. */
function Waiting({ failure, children }: { failure: string | null; children: string }) {
  return (
    <div className="flex h-dvh flex-col">
      <header className="flex min-h-12 items-center gap-3.5 border-b border-line bg-surface px-4 pt-[max(6px,env(safe-area-inset-top))] pb-1.5">
        <strong className="mr-auto text-sm font-semibold">Multiplayer</strong>
        <Button tone="quiet" size="sm" onClick={() => location.assign(START.href)}>
          {START.label}
        </Button>
      </header>
      <Notice tone={failure ? "error" : "quiet"}>{failure ?? children}</Notice>
    </div>
  );
}

/** `?online`: makes a table on the server and goes to its address, as its host. */
function NewTable({ start }: { start: OnlineStart }) {
  const [failure, setFailure] = useState<string | null>(null);
  useEffect(() => {
    const request = tableRequest(start, {
      nickname: keptNickname(localStorage),
      players: Number(localStorage.getItem(PLAYER_COUNT_KEY) || Number.NaN),
    });
    created ??= createTable(request);
    created
      .then((made) => {
        keepPlayer(localStorage, made.gameId, made.playerId, made.playerSession);
        localStorage.setItem(NICKNAME_KEY, request.nickname);
        if (!start.fill) {
          localStorage.setItem(PLAYER_COUNT_KEY, String(request.playerCount));
        }
        // A page load: the address is the table and the seat, so a reload goes on with them.
        location.replace(tableAddress(made.gameId, made.playerId));
      })
      .catch((error: unknown) => setFailure(message(error)));
  }, [start]);
  return <Waiting failure={failure}>Making a table on the server…</Waiting>;
}

/** The running game of a table, as the player of this tab, or as a spectator. */
function OnlineGame({
  engine,
  gameId,
  seat,
}: {
  engine: Engine;
  gameId: string;
  seat: Seat | null;
}) {
  const [transport, setTransport] = useState<Transport | null>(null);
  // Made in an effect, as the game of `LocalApp`: each mount gets a connection of its own.
  useEffect(() => {
    const made = createWsTransport(engine, {
      gameId,
      playerSession: seat?.playerSession ?? null,
      // What a page of before kept in this browser goes to the server once.
      legacyStaged: seat ? legacyStaged(localStorage, gameId, seat.playerId) : undefined,
    });
    setTransport(made);
    return () => {
      made.close();
      setTransport(null);
    };
  }, [engine, gameId, seat]);
  // What is staged is kept by the server for the seat: every tab of the seat has the same.
  const { session, error } = useLiveSession(transport, transport?.staged);
  // The table of the game has a socket of its own, open while the sheet "Table" is in view.
  const openTable = useCallback(() => serverTable(gameId, seat), [gameId, seat]);
  const takeOver = useCallback(
    (table: Table, player: string, name: string) => rejoin(table, gameId, player, name),
    [gameId],
  );
  if (session && !error) {
    return (
      <div className="flex h-dvh flex-col">
        <div className="relative grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)]">
          <GameShell
            session={session}
            other={START}
            table={
              <GameTable
                open={openTable}
                catalog={engine.catalog}
                // Only a visitor sits down in a seat: a player has one.
                onTakeOver={seat ? undefined : takeOver}
              />
            }
          />
        </div>
      </div>
    );
  }
  return <Waiting failure={error}>Connecting to the game…</Waiting>;
}

/** Who this tab is at the table: a player that the browser kept, or a visitor. */
function useSeat(gameId: string, asked: string | null): { seat: Seat | null } | null {
  const [who, setWho] = useState<{ seat: Seat | null } | null>(null);
  useEffect(() => {
    let stale = false;
    const resolve = () => {
      if (stale) {
        return;
      }
      const mine = playersOf(localStorage, gameId);
      const ids = Object.keys(mine);
      // Without `seat`, the one player of this browser is the tab; of several, none is.
      const id = asked !== null ? asked : ids.length === 1 ? ids[0] : null;
      const seat = id !== null && mine[id] ? { playerId: id, playerSession: mine[id] } : null;
      if ((seat?.playerId ?? null) !== asked) {
        history.replaceState(null, "", tableAddress(gameId, seat?.playerId));
      }
      setWho({ seat });
    };
    // An older page kept one player session for the game, with no id: the server says whose.
    const legacy = legacySession(localStorage, gameId);
    if (legacy === null) {
      resolve();
    } else {
      playerOf(gameId, legacy)
        .then((id) => id !== null && moveLegacy(localStorage, gameId, id))
        .catch(() => {})
        .then(resolve);
    }
    return () => {
      stale = true;
    };
  }, [gameId, asked]);
  return who;
}

/**
 * A table on the server, at the address of the page (`/api`, `/ws`): `?online=<game_id>` is the
 * table, and `&seat=<player_id>` says which player of this browser the tab is. Before the game
 * the page is the lobby; when the table says that the game runs, the page is the game, as that
 * player. The printed cards are those of the engine of this page.
 */
function TableApp({ gameId, asked }: { gameId: string; asked: string | null }) {
  const [engine, setEngine] = useState<Engine | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const who = useSeat(gameId, asked);
  const seat = who?.seat ?? null;
  const [table, setTable] = useState<Table | null>(null);
  const [state, setState] = useState<TableState | null>(null);
  const [offline, setOffline] = useState(false);
  const [toast, setToast] = useState<{ id: number; text: string } | null>(null);
  const [inspected, setInspected] = useState<string | null>(null);
  const [bot, chooseBot] = useChosenBot();
  const [mine, setMine] = useState<string[]>([]);
  const [nickname, setNickname] = useState(
    () => localStorage.getItem(NICKNAME_KEY) ?? DEFAULT_NICKNAME,
  );
  const running = state?.lobby.phase === "running";
  /** A visitor of a running game chose "Watch": the page is the game, with no seat. */
  const [watching, setWatching] = useState(false);
  // A player of a running game is in the game. A visitor is shown the table first.
  const inGame = running && (seat !== null || watching);

  useEffect(() => {
    let stale = false;
    loadEngine(wasmUrl)
      .then((loaded) => stale || setEngine(loaded))
      .catch((error: unknown) => stale || setFailure(message(error)));
    return () => {
      stale = true;
    };
  }, []);

  // The time is the id: the screen has messages of its own, and shows the newest.
  const say = useCallback((text: string) => setToast({ id: performance.now(), text }), []);
  // The socket of the table is open while the page shows the table: the game has its own.
  useEffect(() => {
    if (!who || inGame) {
      return;
    }
    setMine(Object.keys(playersOf(localStorage, gameId)));
    const made = serverTable(gameId, who.seat);
    let before: TableState["lobby"] | null = null;
    const stop = made.subscribe((event) => {
      switch (event.kind) {
        case "table": {
          const { lobby, viewer } = event;
          if (who.seat && viewer === null && lobby.phase === "lobby") {
            // The table no longer has the player of this tab: the tab is a visitor.
            dropPlayer(localStorage, gameId, who.seat.playerId);
            return location.replace(tableAddress(gameId));
          }
          // A change of the host is told to the others once.
          const changed =
            before && viewer !== lobby.host_player_id ? hostChange(before, lobby) : null;
          if (changed) {
            say(changed);
          }
          before = lobby;
          setOffline(false);
          return setState(event);
        }
        case "offline":
          return setOffline(true);
        case "rejected":
          return say(event.reason);
        case "error":
          return setFailure(event.message);
      }
    });
    setTable(made);
    return () => {
      stop();
      made.close();
      setTable(null);
    };
  }, [gameId, who, inGame, say]);

  const view = useMemo(
    () => state && selectLobby(state, { mine, inspected, offline, bot }, engine?.catalog ?? null),
    [state, mine, inspected, offline, engine, bot],
  );

  if (failure) {
    return <Waiting failure={failure}>…</Waiting>;
  }
  if (!state || !view) {
    return <Waiting failure={null}>Connecting to the table…</Waiting>;
  }
  if (inGame) {
    return engine ? (
      <OnlineGame engine={engine} gameId={gameId} seat={seat} />
    ) : (
      <Waiting failure={null}>Loading the game…</Waiting>
    );
  }

  const dispatch = (intent: LobbyIntent) => {
    switch (intent.type) {
      case "inspectSystem":
        return setInspected(intent.system);
      case "closeInspector":
        return setInspected(null);
      case "join": {
        const name = nickname.trim();
        if (!validNickname(name)) {
          return;
        }
        localStorage.setItem(NICKNAME_KEY, name);
        return void table?.join(name).then((joined) => {
          if (joined) {
            keepPlayer(localStorage, gameId, joined.playerId, joined.playerSession);
            location.replace(tableAddress(gameId, joined.playerId));
          }
        });
      }
      case "joinTab": {
        if (!table || state.viewer === null) {
          return;
        }
        // The tab is opened with the click: a browser blocks one that is opened later.
        const tab = window.open("", "_blank");
        return void table.join(nextNickname(state.lobby, state.viewer)).then((joined) => {
          if (!joined) {
            return tab?.close();
          }
          keepPlayer(localStorage, gameId, joined.playerId, joined.playerSession);
          setMine(Object.keys(playersOf(localStorage, gameId)));
          const address = tableAddress(gameId, joined.playerId);
          if (tab) {
            tab.location.replace(address);
          } else {
            say("The browser did not open a tab.");
          }
        });
      }
      case "become":
        return location.assign(tableAddress(gameId, intent.player));
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
      case "rename":
        if (!validNickname(intent.name)) {
          return;
        }
        localStorage.setItem(NICKNAME_KEY, intent.name);
        return table?.rename(intent.name);
      case "faction":
        return table?.setFaction(intent.player, intent.faction);
      case "start":
        return table?.start();
      case "watch":
        return setWatching(true);
      case "takeOver":
        return table ? rejoin(table, gameId, intent.player, intent.name) : undefined;
    }
  };

  return (
    <div className="flex h-dvh flex-col">
      <div className="relative grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)]">
        <LobbyScreen
          view={view}
          dispatch={dispatch}
          nickname={nickname}
          onNickname={setNickname}
          validName={validNickname}
          link={`${location.origin}/${tableAddress(gameId)}`}
          toast={toast}
          other={START}
        />
      </div>
    </div>
  );
}

export function OnlineApp({
  gameId,
  seat,
  start,
}: {
  /** Null: `?online` with no game, which makes a table. */
  gameId: string | null;
  /** The player of this tab, from the address. */
  seat: string | null;
  start: OnlineStart;
}) {
  return gameId === null ? <NewTable start={start} /> : <TableApp gameId={gameId} asked={seat} />;
}
