import {
  type ChangeEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { SeatShape } from "../game/context";
import { GameShell } from "../game/shell/GameShell";
import { DEFAULT_GAME } from "./defaultGame";
import { DemoApp } from "./DemoApp";
import { GameTable } from "./GameTable";
import { DEFAULT_NICKNAME, onlineAddress } from "./onlineStart";
import {
  type LocalGame,
  STEPPING_KEY,
  gameOfSave,
  clearLeft,
  gameAddress,
  idAddress,
  readWait,
  sideKey,
  slotOf,
  storeLeft,
  storeWait,
} from "../session/savedGame";
import { localGameTable } from "../session/localTable";
import {
  ModelMissing,
  type ServedModel,
  filesOfTable,
  megabytes,
  workerFiles,
} from "../session/models";
import { keptNickname } from "../session/onlineKeys";
import {
  type GameMeta,
  type OfflineSaveStore,
  type SavedGameRecord,
  autosaveOf,
  engineOfSave,
  factionsOf,
  importFailure,
  newSaveId,
  removeSave,
  saveStore,
  storeWarning,
  takeLeft,
} from "../session/saves";
import type { Transport } from "../session/transport";
import type { ReactionModes, SeatSwitch, SessionUpdate } from "../session/wire";
import { useLiveSession } from "../session/useLiveSession";
import {
  type Engine,
  type HostSave,
  createWasmTransport,
  loadEngine,
} from "../session/wasmTransport";
import { Button, Drawer, Hint, Segmented } from "../ui";

// Not an import: the file is built by scripts/build-wasm.sh and is not in the repository. Without
// it the rest of web2 still builds, and this page says how to build it.
export const wasmUrl = new URL("../session/ti4.wasm", import.meta.url).href;

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The name of the player, for the saved games. */
const nickname = () => keptNickname(localStorage) ?? DEFAULT_NICKNAME;

/** What the page starts from: the engine, the save store, and the saved game of this address if there is one. */
type Start =
  | { kind: "loading" }
  | { kind: "failed"; message: string }
  /** The address names a saved game by its id, and this browser has none of that id. */
  | { kind: "missing" }
  /** A model that the game seats cannot be had: the game cannot go on. */
  | { kind: "nomodel"; message: string; store: OfflineSaveStore; id: string | null }
  | {
      kind: "ready";
      engine: Engine;
      store: OfflineSaveStore;
      /** The id of the game in the store: of the saved game, or a new one. */
      id: string;
      record: SavedGameRecord | null;
      /** The files of the models that the seats of the game play, for its Worker. */
      files: { model: ServedModel; bytes: Uint8Array }[];
    };

/**
 * A game of the engine in this page. `?local=3&players=8&humans=1`: the seed, the number of
 * seats, and which seats are played here (bit 0 is seat A). `?scenario=late-game` starts from a
 * prepared board of the engine. The decisions of every seat are kept in the save store under the
 * slot of the address (or, with `id`, under that id), so a reload goes on where the game was.
 */
export function LocalApp({
  game,
  id: savedId,
  onNewGame,
  other = { label: "Demo", href: "/?example=live-strategy-3" },
}: {
  game: LocalGame;
  /** The saved game of this id (`?save=<id>`), in place of the game of the slot of `game`. */
  id?: string;
  /**
   * "New game" leaves the game for what set it up, in place of the same game again: the game of
   * a table goes back to its setup.
   */
  onNewGame?: () => void;
  /** The way out of the game, in the header. */
  other?: { label: string; href: string };
}) {
  const [start, setStart] = useState<Start>({ kind: "loading" });
  /** How far the file of a model of the game is, while it loads. */
  const [loading, setLoading] = useState<string | null>(null);
  /** The saved game was played by another engine, and the player said to play it anyway. */
  const [anyway, setAnyway] = useState(false);
  /** How many decisions of the game are saved, those of every seat, and how many are answers of this page. */
  const [decisions, setDecisions] = useState(0);
  const [answers, setAnswers] = useState(0);
  /** The seat changes of the game ("Play as"). The last one says which seats are played here. */
  const [switches, setSwitches] = useState<readonly SeatSwitch[]>([]);
  const [importError, setImportError] = useState<string | null>(null);
  /** Why the saved games are kept in memory only, in words. */
  const [warning, setWarning] = useState<string | null>(null);
  const [transport, setTransport] = useState<Transport | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // A scenario names its seed and its seats, and the engine knows them.
  const scenarios = start.kind === "ready" ? start.engine.scenarios : null;
  const played = useMemo(() => {
    const scenario = scenarios?.find((item) => item.id === game.scenario);
    return scenario ? { ...game, seed: scenario.seed, players: scenario.players } : game;
  }, [game, scenarios]);
  /** The other seats wait for "Step" before each decision. A setting of the page. */
  const [stepping, setStepping] = useState(() => localStorage.getItem(STEPPING_KEY) === "1");
  // The game that is played is told, and the next one starts with it.
  const steps = useRef(stepping);
  steps.current = stepping;
  /**
   * The wait of the seats that the page plays, as the host set it last: in the setup, or in the
   * sheet "Table" of the game. The game that is played is told, and localStorage keeps it next to
   * the saved game.
   */
  const wait = useRef(game.waitMs ?? 0);
  /** The writes of the saves of the game that is played. */
  const saving = useRef<{ flush(): Promise<void>; drop(): Promise<void> } | null>(null);
  // Without the shell nothing else closes the Dev sheet: Esc does, as in the shell.
  useEffect(() => {
    if (!settingsOpen) {
      return;
    }
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && setSettingsOpen(false);
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [settingsOpen]);

  useEffect(() => {
    let stale = false;
    Promise.all([loadEngine(wasmUrl), saveStore()])
      .then(async ([engine, store]): Promise<Start> => {
        // A page that was left before the store had its last save kept it in localStorage.
        await takeLeft(store, localStorage, nickname());
        const record =
          savedId === undefined ? await store.findSlot(slotOf(game)) : await store.load(savedId);
        if (savedId !== undefined && !record) {
          return { kind: "missing" };
        }
        // The models of the game are loaded before it starts (a saved game too).
        const table = record ? gameOfSave(record.save as HostSave).table : game.table;
        let files: { model: ServedModel; bytes: Uint8Array }[];
        try {
          files = await filesOfTable(table, (model, done, total) => {
            if (!stale) {
              setLoading(`Loading ${model.label} · ${megabytes(done, total)}`);
            }
          });
        } catch (error) {
          return {
            kind: "nomodel",
            store,
            id: record?.id ?? null,
            message:
              error instanceof ModelMissing
                ? error.message
                : `A model that this game seats did not load. ${message(error)}`,
          };
        }
        return { kind: "ready", engine, store, id: record?.id ?? newSaveId(), record, files };
      })
      .catch((error: unknown) => ({ kind: "failed" as const, message: message(error) }))
      .then((next) => stale || setStart(next));
    return () => {
      stale = true;
    };
  }, [game, savedId]);

  const store = start.kind === "ready" ? start.store : null;
  useEffect(() => {
    if (!store) {
      return;
    }
    const tell = () => setWarning(storeWarning(store.status()));
    tell();
    return store.subscribe(tell);
  }, [store]);

  /** The saved game that the page plays again, and the engine that played it. */
  const saved = useMemo(
    () =>
      start.kind === "ready" && start.record
        ? { save: start.record.save as HostSave, engine: start.record.engine.wasmSha256 }
        : null,
    [start],
  );
  const foreign = start.kind === "ready" && saved !== null && saved.engine !== start.engine.hash;
  const waits = foreign && !anyway;

  // Made in an effect, not in a memo: the game is closed when the page lets go of it, and React
  // may do that and mount again (StrictMode). Each mount gets a game of its own.
  useEffect(() => {
    if (start.kind !== "ready" || waits) {
      return;
    }
    const { engine, store, id, record } = start;
    const save = saved?.save;
    if (save) {
      wait.current = readWait(localStorage, id);
    } else {
      wait.current = game.waitMs ?? 0;
      storeWait(localStorage, id, wait.current);
    }
    setDecisions(save?.decisions.length ?? 0);
    setAnswers(save?.answers.length ?? 0);
    setSwitches(save?.switches ?? []);
    /** A write of the store failed since the last flush: what localStorage keeps stays. */
    let failed = false;
    const autosave = autosaveOf(store, () => {
      failed = true;
    });
    /** The game was removed: what it saves from now on is not kept. */
    let gone = false;
    let factions = record?.summary.factions ?? null;
    const slot = savedId === undefined ? slotOf(game) : undefined;
    const meta = (made: HostSave): GameMeta => ({
      nickname: nickname(),
      factions: factions?.length === made.players ? factions : factionsOf(null, made.players),
      engine: engineOfSave(made, engine.hash),
      // A game of an address keeps its slot; a game opened by its id keeps what it has.
      ...(slot === undefined ? {} : { slot }),
    });
    /** The newest save of the game. */
    let latest: HostSave | null = null;
    /** localStorage keeps a save of the game that the store may not have. */
    let kept = false;
    /** Removes the save of localStorage when the store has the newest save. */
    const settle = () => {
      const at = latest;
      failed = false;
      void autosave.flush().then(() => {
        if (kept && latest === at && !failed && !gone) {
          clearLeft(localStorage, id);
          kept = false;
        }
      });
    };
    const write = (next: HostSave) => {
      latest = next;
      autosave.schedule(id, next, meta(next));
      if (kept) {
        settle();
      }
    };
    const made = createWasmTransport(engine, played, {
      save,
      models: workerFiles(start.files),
      keepFirst: game.table !== undefined,
      stepping: steps.current,
      waitMs: wait.current,
      modes: readModes(id),
      onModes: (modes) => localStorage.setItem(sideKey(id, "cards"), JSON.stringify(modes)),
      onSave(next) {
        if (gone) {
          return;
        }
        write(next);
        setDecisions(next.decisions.length);
        setAnswers(next.answers.length);
        setSwitches(next.switches ?? []);
      },
      onLeave(next) {
        if (gone) {
          return;
        }
        // The page may be gone before the store has it: localStorage takes it at once.
        kept =
          storeLeft(localStorage, id, {
            ...(slot ? { slot } : {}),
            engine: engine.hash,
            at: Date.now(),
            factions: meta(next).factions,
            save: next,
          }) || kept;
        latest = next;
        settle();
      },
    });
    const stop = made.subscribe((event) => {
      if (event.kind === "update") {
        factions = factionsOf(event.update as SessionUpdate, event.update.view.players.length);
      }
    });
    saving.current = {
      flush: () => autosave.flush(),
      async drop() {
        gone = true;
        await autosave.flush();
      },
    };
    setTransport(made);
    return () => {
      stop();
      made.close();
      autosave.dispose();
      saving.current = null;
      setTransport(null);
    };
  }, [start, saved, waits, played, game, savedId]);

  const stepSeats = (on: boolean) => {
    setStepping(on);
    localStorage.setItem(STEPPING_KEY, on ? "1" : "0");
    transport?.capabilities.stepping?.set(on);
  };
  const { session, error, replaying } = useLiveSession(transport, transport?.staged);
  // The table of the game, for its sheet: of its setup the wait can still change.
  const module = start.kind === "ready" ? start.engine.module : null;
  const catalog = start.kind === "ready" ? start.engine.catalog : null;
  const id = start.kind === "ready" ? start.id : null;
  const openTable = useCallback(
    () =>
      localGameTable(module!, game, {
        waitMs: wait.current,
        onWait(ms) {
          wait.current = ms;
          transport?.capabilities.wait?.(ms);
          // The wait is kept next to the saved game: a reload waits as long.
          if (id !== null) {
            storeWait(localStorage, id, ms);
          }
        },
      }),
    [module, game, transport, id],
  );
  // The shell takes an open Dev sheet over. A page without the shell starts with it closed.
  const shown = !waits && !!session;
  useEffect(() => {
    if (shown) {
      setSettingsOpen(false);
    }
  }, [shown]);

  /** The seats that are played here (bit 0 is seat A). */
  const humans = switches.at(-1)?.humans ?? game.humans;
  const playsHere = (seat: string) => (humans >> (seat.charCodeAt(0) - 97)) & 1;
  /** Plays `seat` here in place of the seat that is shown. The engine drops what that seat staged. */
  const playAs = (seat: string) => transport?.capabilities.switchSeat?.(seat);
  const newGame = async () => {
    setSettingsOpen(false);
    if (start.kind === "nomodel") {
      // The game cannot go on without its model: it is dropped, as New game drops a game.
      if (start.id !== null) {
        await removeSave(start.store, localStorage, start.id);
      }
      if (onNewGame) {
        return onNewGame();
      }
      window.location.search = gameAddress(game);
      return;
    }
    if (start.kind !== "ready") {
      return;
    }
    // What the game would still save is not kept: the game is gone.
    await saving.current?.drop();
    await removeSave(start.store, localStorage, start.id);
    setAnyway(false);
    if (onNewGame) {
      return onNewGame();
    }
    if (savedId !== undefined) {
      // A game of an id has no address of its own to start again at: the address of its setup.
      window.location.search = gameAddress(game);
      return;
    }
    setStart({ ...start, id: newSaveId(), record: null });
  };
  const exportGame = async () => {
    if (start.kind !== "ready") {
      return;
    }
    try {
      await saving.current?.flush();
      const { filename, blob } = await start.store.exportGame(start.id);
      const link = document.createElement("a");
      link.href = URL.createObjectURL(blob);
      link.download = filename;
      link.click();
      URL.revokeObjectURL(link.href);
    } catch {
      setImportError("The game is not saved yet.");
    }
  };
  const importGame = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file || start.kind !== "ready") {
      return;
    }
    const text = await file.text();
    try {
      const imported = await start.store.importGame(text);
      // An imported game is a game of its own, with an id and no address of a setup.
      window.location.search = idAddress(imported.id);
    } catch (failure) {
      setImportError(importFailure(failure, text));
    }
  };
  const file = useRef<HTMLInputElement>(null);

  // A game that starts from a scenario, or from round 1, is another game with its own saved
  // decisions: its address opens it, as Import does.
  const startRow = (id: string | undefined, title: string, description: string) => (
    <div
      key={id ?? ""}
      className="row flex min-h-9 items-center gap-2 pr-2.5 [.row+&]:border-t [.row+&]:border-line/50"
    >
      <button
        type="button"
        className="flex min-h-9 flex-1 cursor-pointer items-center gap-2.5 px-3.5 text-left text-base hover:bg-cyan/[.06]"
        aria-current={game.scenario === id}
        onClick={() => {
          if (game.scenario !== id) {
            window.location.search = gameAddress(
              id === undefined
                ? { seed: game.seed, players: game.players, humans: game.humans }
                : { ...game, scenario: id },
            );
          }
        }}
      >
        <strong className="font-mid">{title}</strong>
        {game.scenario === id && <small className="text-xs text-muted">✓ Selected</small>}
      </button>
      <Hint label={`About ${title}`}>{description}</Hint>
    </div>
  );

  // What the game can do besides play: each part of the Dev sheet is shown from it. Before there
  // is a game, the sheet has every part, as a local game does.
  const made = transport?.capabilities;
  const can = {
    stepping: !made || !!made.stepping,
    switchSeat: !made || !!made.switchSeat,
    scenarios: !made || !!made.scenarios,
    save: !made || !!made.save,
  };
  // The Dev sheet. The shell shows it in its sheet slot; a page without the shell shows it below.
  const settingsView = (close: () => void) => (
    <>
      {warning && (
        <p role="alert" className="mb-3 text-orange">
          ⚠ {warning}
        </p>
      )}
      <p className="text-muted">
        {`${game.table ? "Single player" : "Local game"} · ${game.scenario === undefined ? "" : `scenario ${game.scenario} · `}seed ${played.seed} · ${played.players} seats · ${answers} answers saved · ${decisions} decisions of all seats`}
      </p>
      {importError && (
        <p role="alert" className="mt-3 text-red">
          {importError}
        </p>
      )}
      {can.stepping && (
        <div className="mt-4 flex flex-wrap items-center gap-2.5">
          <span className="text-sm text-muted">Other seats</span>
          <Segmented
            label="Other seats"
            options={[
              { id: "run", label: "Run" },
              { id: "step", label: "Step" },
            ]}
            value={stepping ? "step" : "run"}
            onChange={(mode) => stepSeats(mode === "step")}
          />
          <Hint label="About the other seats">
            The other seats decide at random. With Step the game waits before each of their
            decisions, and the Step button of the action panel lets the seat decide.
          </Hint>
        </div>
      )}
      {session && can.switchSeat && (
        <section aria-label="Play as" className="mt-4">
          <h3 className="flex items-center gap-2 text-sm text-muted">
            Play as
            <Hint label="About Play as">
              You play the chosen seat from now on. The seat that you leave decides at random, from
              its open decision on. Undo does not go back before the change.
            </Hint>
          </h3>
          <div className="mt-1.5 rounded border border-line">
            {Object.values(session.view.seats).map((seat) => (
              <button
                key={seat.id}
                type="button"
                className="row flex min-h-9 w-full cursor-pointer items-center gap-2.5 px-3.5 text-left text-base hover:bg-cyan/[.06] disabled:cursor-default disabled:hover:bg-transparent [.row+&]:border-t [.row+&]:border-line/50"
                aria-current={!!playsHere(seat.id)}
                disabled={!!playsHere(seat.id) || !!error || !!replaying}
                onClick={() => {
                  playAs(seat.id);
                  close();
                }}
              >
                <svg viewBox="-6 -6 12 12" className="size-[1em] shrink-0" aria-hidden="true">
                  <SeatShape symbol={seat.symbol} x={0} y={0} size={11.2} color={seat.color} />
                </svg>
                <strong className="font-mid">{seat.name}</strong>
                <span className="min-w-0 flex-1 truncate text-muted">{seat.faction}</span>
                <small className="text-xs text-muted">
                  {playsHere(seat.id) ? "✓ Playing" : game.table ? "Played by the page" : "Random"}
                </small>
              </button>
            ))}
          </div>
        </section>
      )}
      {scenarios && can.scenarios && !game.table && (
        <section aria-label="Start from" className="mt-4">
          <h3 className="text-sm text-muted">Start from</h3>
          <div className="mt-1.5 rounded border border-line">
            {startRow(undefined, "Round 1", "A new game from its setup, on the seed of this page.")}
            {scenarios.map((scenario) =>
              startRow(scenario.id, scenario.title, scenario.description),
            )}
          </div>
        </section>
      )}
      <section
        aria-label="Multiplayer"
        className="mt-4 flex min-h-9 items-center gap-2 rounded border border-line pr-2.5"
      >
        <button
          type="button"
          className="flex min-h-9 flex-1 cursor-pointer items-center gap-2.5 px-3.5 text-left text-base hover:bg-cyan/[.06]"
          onClick={() => {
            // A page load: the game on the server is another game, with an address of its own.
            window.location.search = onlineAddress({ seed: played.seed, fill: "random" });
          }}
        >
          <strong className="font-mid">Multiplayer</strong>
          <small className="text-xs text-muted">{`seed ${played.seed} · 8 seats`}</small>
        </button>
        <Hint label="About Multiplayer">
          A new game of eight on the server, on the seed of this page. You play the first seat, and
          the other seats decide at random.
        </Hint>
      </section>
      {can.save && (
        <div className="mt-4 flex flex-wrap gap-2">
          <Button
            tone="quiet"
            size="sm"
            disabled={decisions === 0 && !onNewGame}
            onClick={() => {
              newGame();
              close();
            }}
          >
            New game
          </Button>
          <Button tone="quiet" size="sm" disabled={decisions === 0} onClick={exportGame}>
            Export
          </Button>
          <Button tone="quiet" size="sm" onClick={() => file.current?.click()}>
            Import
          </Button>
        </div>
      )}
    </>
  );

  // The engine did not load: the demo is shown with the reason, so the page is never empty.
  if (start.kind === "failed") {
    return <DemoApp notice={`Local game unavailable. ${start.message}`} />;
  }

  let body: ReactNode;
  if (start.kind === "missing") {
    body = (
      <Notice tone="error">
        This browser has no saved game of this address. The saved games are listed on the{" "}
        <a href="/" className="text-cyan underline">
          start screen
        </a>
        .
      </Notice>
    );
  } else if (start.kind === "nomodel") {
    body = (
      <Notice tone="error">
        {start.message}
        <span className="mt-3 flex gap-2">
          <Button onClick={newGame}>New game</Button>
        </span>
      </Notice>
    );
  } else if (start.kind === "loading" && loading) {
    body = <Notice tone="quiet">{loading}</Notice>;
  } else if (waits) {
    body = (
      <Notice tone="error">
        The saved game ({saved?.save.decisions.length ?? 0} decisions) was played by another build
        of the engine. This build may not offer the same options.
        <span className="mt-3 flex gap-2">
          <Button onClick={() => setAnyway(true)}>Replay anyway</Button>
          <Button onClick={newGame}>New game</Button>
        </span>
      </Notice>
    );
  } else if (!session) {
    body = (
      <Notice tone={error ? "error" : "quiet"}>
        {error ??
          (replaying
            ? `Playing the saved game again: decision ${replaying.done} of ${replaying.total}…`
            : "Starting the game…")}
      </Notice>
    );
  } else {
    body = (
      <GameShell
        session={session}
        other={other}
        settings={settingsView}
        settingsOpen={settingsOpen}
        table={game.table && module ? <GameTable open={openTable} catalog={catalog} /> : undefined}
      />
    );
  }

  // The shell has the toolbar, and the toolbar has the Dev entry. Without the shell the page
  // has a row of its own, so Import and New game stay in reach.
  const inShell = shown;
  return (
    <div className="flex h-dvh flex-col">
      {!inShell && (
        <header className="flex min-h-12 items-center gap-3.5 border-b border-line bg-surface px-4 pt-[max(6px,env(safe-area-inset-top))] pb-1.5">
          <strong className="mr-auto text-sm font-semibold">Local game</strong>
          <Button tone="quiet" size="sm" onClick={() => setSettingsOpen(true)}>
            Dev
          </Button>
        </header>
      )}
      <div className="relative grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)]">
        {body}
        {!inShell && settingsOpen && (
          <Drawer title="Dev" onClose={() => setSettingsOpen(false)}>
            {settingsView(() => setSettingsOpen(false))}
          </Drawer>
        )}
      </div>
      <input
        ref={file}
        type="file"
        accept="application/json,.json"
        aria-label="Saved game file"
        hidden
        onChange={importGame}
      />
    </div>
  );
}

/**
 * The modes of the reactions that the player set in this game, as they were kept. A page of
 * before the modes of abilities kept the names of the cards that are never offered.
 */
function readModes(id: string): ReactionModes {
  try {
    const kept: unknown = JSON.parse(localStorage.getItem(sideKey(id, "cards")) || "{}");
    const modes: ReactionModes = {};
    if (Array.isArray(kept)) {
      for (const name of kept) {
        if (typeof name === "string") {
          modes[name] = "never";
        }
      }
    } else if (kept && typeof kept === "object") {
      for (const [key, mode] of Object.entries(kept)) {
        if (mode === "never" || mode === "ownturn") {
          modes[key] = mode;
        }
      }
    }
    return modes;
  } catch {
    return {};
  }
}

export function Notice({ tone, children }: { tone: "quiet" | "error"; children: ReactNode }) {
  return (
    <p role="status" className={tone === "error" ? "p-5 text-red" : "p-5 text-muted"}>
      {children}
    </p>
  );
}

export { DEFAULT_GAME };

/**
 * The game that the address asks for, or none: the bare address is the start screen, which
 * opens DEFAULT_GAME by its address. With `scenario`, the engine says which seed and how many
 * seats the game has.
 */
export function localGame(params: URLSearchParams): LocalGame | null {
  const number = (name: string, otherwise: number) => {
    const value = Number(params.get(name) || otherwise);
    return Number.isInteger(value) && value >= 0 ? value : otherwise;
  };
  const scenario = params.get("scenario");
  if (scenario) {
    return { ...DEFAULT_GAME, humans: number("humans", 1), scenario };
  }
  if (!params.has("local")) {
    return null;
  }
  return { seed: number("local", 3), players: number("players", 8), humans: number("humans", 1) };
}
