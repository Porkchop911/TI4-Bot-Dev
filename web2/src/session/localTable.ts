// The table of single player: it is kept in the page, and the engine of the page says what it can
// be (`ti4_host::setup` through the setup exports), as the server says it for the table of a
// lobby. The page is its host and its one person; the other seats are played by the page.
import {
  type ModelProgress,
  type ServedModel,
  modelEntries,
  modelFile,
  modelOfKind,
} from "./models";
import type { LocalGame } from "./savedGame";
import type { Table, TableEvent, TableSettings } from "./table";
import { setupDrawOf, setupOptionsOf, setupPreviewOf } from "./wasmTransport";
import type {
  LobbyState,
  MapChoice,
  MapTemplateSummary,
  SeatRequest,
  TableOptions,
  TablePreview,
  TableRequest,
} from "./wire";

/** The ids of the players, in the order of their seats (`ti4-wasm`, `SEATS`). */
const PLAYERS = "abcdefgh";
/** The player of the page, the host: the id of the seat of the person. */
const viewerOf = (seats: { kind: Kind }[]) =>
  PLAYERS[
    Math.max(
      0,
      seats.findIndex((seat) => seat.kind === "human"),
    )
  ];
/** The kind of a seat that is added with the count: the usual choice. */
const USUAL_KIND = "smart";

export interface LocalTablePlay {
  /** The name of the player of the page. */
  nickname: string;
  /** The number of seats that the table begins with. */
  players: number;
  /** The seed of the table: its seats, its factions and its game are drawn from it. */
  seed: number;
  /** The host started the game: what plays it, and plays it again. */
  onStart(game: LocalGame): void;
  /** The models that the server offers (`GET /api/models`): each is the kind `mlp:<name>`. */
  models?: readonly ServedModel[];
  /** Loads the file of a model into the Cache API (`modelFile`); a test gives its own. */
  loadModel?(model: ServedModel, progress: ModelProgress): Promise<unknown>;
}

/** Who plays a seat. Null: the seat is open. */
type Kind = SeatRequest["kind"] | null;

/** What the host chose. Everything that the table shows is made from it. */
interface Chosen {
  seats: { kind: Kind; faction: string | null }[];
  /** Null: the usual set. */
  cards: string | null;
  /** Null: the map that a table of this size starts with. */
  map: MapChoice | null;
  waitMs: number;
  startFrom: string | null;
}

/** A change that the table does not take. The text is told as it is. */
class Refused extends Error {}

/**
 * A table in the page. Each change is checked by the engine and applies at once; a change that
 * the engine refuses is told with its reason, and the table stays as it was.
 */
export function localTable(module: WebAssembly.Module, play: LocalTablePlay): Table {
  const listeners = new Set<(event: TableEvent) => void>();
  const options = new Map<number, Promise<TableOptions>>();
  const optionsOf = (count: number) => {
    let asked = options.get(count);
    if (!asked) {
      asked = setupOptionsOf(module, count, modelEntries(play.models ?? []));
      options.set(count, asked);
    }
    return asked;
  };
  let chosen: Chosen = {
    seats: Array.from({ length: play.players }, (_, place) => ({
      kind: place === 0 ? "human" : USUAL_KIND,
      faction: null,
    })),
    cards: null,
    map: null,
    waitMs: 0,
    startFrom: null,
  };
  let latest: Extract<TableEvent, { kind: "table" }> | null = null;
  /** The table as the engine reads it, as it was shown last. */
  let request: TableRequest | null = null;
  let version = 0;
  let running = false;
  let closed = false;
  /** The changes apply one after the other: each is checked against the table before it. */
  let queue: Promise<void> = Promise.resolve();
  /** The models whose files are loaded, by name. */
  const loaded = new Set<string>();
  /** The files that load, by the name of their model: how far each is. */
  const loading = new Map<string, { label: string; done: number; total: number }>();

  const say = (event: TableEvent) => {
    for (const listener of listeners) {
      listener(event);
    }
  };
  /** The names of the models that the seats play, in the order of the seats. */
  const seatedModels = () => [
    ...new Set(chosen.seats.flatMap((seat) => modelOfKind(seat.kind) ?? [])),
  ];
  /** The first file of the table that is not loaded yet. */
  const waitingFile = () => {
    for (const name of seatedModels()) {
      const file = loading.get(name);
      if (file) {
        return file;
      }
    }
    return undefined;
  };
  /** Tells the table again with how far its files are. */
  const tellLoading = () => {
    if (!latest || closed) {
      return;
    }
    const file = waitingFile();
    const { loading: _, ...rest } = latest;
    latest = file ? { ...rest, loading: { ...file } } : rest;
    say(latest);
  };
  /** Loads the file of each model that a seat plays and that is not loaded or loading. */
  const loadFiles = () => {
    for (const name of seatedModels()) {
      const model = play.models?.find((item) => item.id === name);
      if (!model || loaded.has(name) || loading.has(name)) {
        continue;
      }
      loading.set(name, { label: model.label, done: 0, total: model.bytes });
      const load = play.loadModel ?? modelFile;
      load(model, (done, total) => {
        loading.set(name, { label: model.label, done, total });
        tellLoading();
      }).then(
        () => {
          loading.delete(name);
          loaded.add(name);
          tellLoading();
        },
        (error: unknown) => {
          loading.delete(name);
          // The seats of the model are open again: the host chooses another kind.
          apply((next) => {
            for (const seat of next.seats) {
              if (modelOfKind(seat.kind) === name) {
                seat.kind = null;
              }
            }
          });
          say({
            kind: "rejected",
            reason: `${model.label} did not load. ${error instanceof Error ? error.message : String(error)}`,
          });
        },
      );
    }
  };

  /** The table of `next` as the engine reads it and as a lobby shows it. Throws what the engine refuses. */
  const build = async (next: Chosen) => {
    const count = next.seats.length;
    const known = await optionsOf(count);
    const scenario = known.scenarios.find((item) => item.id === next.startFrom) ?? null;
    // A map is for one count: a choice that the new count does not have is the usual map again.
    const usual = known.maps.find((map) => map.recommended) ?? known.maps[0];
    const has = (map: MapChoice) =>
      map.kind === "random" || known.maps.some((item) => item.alias === map.alias);
    const map: MapChoice =
      next.map && has(next.map)
        ? next.map
        : usual
          ? { kind: "template", alias: usual.alias }
          : { kind: "random" };
    const cards = next.cards ?? known.strategy_card_sets.find((set) => set.default)?.id;
    const kinds = new Map(known.bot_kinds.map((kind) => [kind.id, kind.name]));
    const numbers = new Map<string, number>();
    /** The name of each player: the person of the page, or the kind of a seat with its number. */
    const names = next.seats.map((seat) => {
      if (seat.kind === null || seat.kind === "human") {
        return play.nickname;
      }
      const number = (numbers.get(seat.kind) ?? 0) + 1;
      numbers.set(seat.kind, number);
      return `${kinds.get(seat.kind) ?? seat.kind} ${number}`;
    });
    const made: TableRequest = {
      setup: {
        player_count: count,
        seed: play.seed,
        map,
        ...(cards === undefined ? {} : { strategy_cards: cards }),
        ...(scenario ? { start_from: scenario.id } : {}),
      },
      // An open seat is no player yet: the engine is asked for the board, which needs none.
      seats: next.seats.map((seat, place) => ({
        kind: seat.kind ?? "human",
        ...(seat.faction && !scenario ? { faction: seat.faction } : {}),
        // The game says who sits in each seat by these names.
        name: names[place],
      })),
    };
    const preview: TablePreview = await setupPreviewOf(module, made);
    // A scenario seats its own factions.
    const seated = scenario ? (await setupDrawOf(module, made)).factions : null;
    const lobby: LobbyState = {
      game_id: "single",
      phase: running ? "running" : "lobby",
      host_player_id: viewerOf(next.seats),
      slots: next.seats.map((seat, place) => {
        const bot = seat.kind !== null && seat.kind !== "human" ? seat.kind : null;
        const faction = seated ? seated[place] : seat.faction;
        return {
          slot_id: `slot_${place + 1}`,
          position: place + 1,
          occupant: seat.kind === null ? null : PLAYERS[place],
          nickname: seat.kind === null ? null : names[place],
          connected: seat.kind !== null,
          can_take_over: false,
          ...(bot ? { bot } : {}),
          ...(seat.kind !== null && faction ? { faction } : {}),
        };
      }),
      lobby_version: version,
      bot_kinds: known.bot_kinds.map((kind) => kind.id),
      map: preview.map,
      map_revision: version,
      strategy_card_set: scenario
        ? (known.strategy_card_sets.find((set) => set.default)?.id ?? "")
        : (cards ?? ""),
      speaker_order: "random",
      factions: known.faction_groups.flatMap((group) => group.factions.map((item) => item.id)),
      bot_delay_ms: next.waitMs,
      ...(scenario ? { start_from: scenario.id } : {}),
    };
    return { made, lobby, preview, known };
  };

  /**
   * Applies a change: `change` gets a copy of what the host chose and changes it, or throws
   * `Refused`. The engine then checks the table that comes of it.
   */
  const apply = (change: (next: Chosen, known: TableOptions) => void) => {
    queue = queue.then(async () => {
      if (closed || running) {
        return;
      }
      const next: Chosen = structuredClone(chosen);
      try {
        change(next, await optionsOf(chosen.seats.length));
        version += 1;
        const { made, lobby, preview, known } = await build(next);
        chosen = next;
        request = made;
        latest = {
          kind: "table",
          lobby,
          viewer: lobby.host_player_id,
          preview,
          options: known,
          local: true,
        };
        loadFiles();
        const file = waitingFile();
        if (file) {
          latest.loading = { ...file };
        }
        say(latest);
      } catch (error) {
        say({ kind: "rejected", reason: error instanceof Error ? error.message : String(error) });
      }
    });
  };
  const placeOf = (playerId: string) => {
    const place = playerId.length === 1 ? PLAYERS.indexOf(playerId) : -1;
    return place >= 0 && place < chosen.seats.length ? place : -1;
  };
  /** Makes the table of `count` seats: the open seats go first, then the seats that the page plays. */
  const resize = (next: Chosen, count: number) => {
    while (next.seats.length < count) {
      next.seats.push({ kind: USUAL_KIND, faction: null });
    }
    while (next.seats.length > count) {
      const open = next.seats.findLastIndex((seat) => seat.kind === null);
      const played = next.seats.findLastIndex((seat) => seat.kind !== "human");
      next.seats.splice(open >= 0 ? open : played, 1);
    }
  };

  // The first table.
  apply(() => {});

  return {
    subscribe(listener) {
      listeners.add(listener);
      if (latest) {
        listener(latest);
      }
      return () => listeners.delete(listener);
    },
    async join() {
      say({ kind: "rejected", reason: "A table in the page has one person." });
      return null;
    },
    async takeOver() {
      say({ kind: "rejected", reason: "A table in the page has one person." });
      return null;
    },
    leave() {},
    addBots(kind, fill = false) {
      apply((next, known) => {
        if (!known.bot_kinds.some((item) => item.id === kind)) {
          throw new Refused(`The page plays no seat of the kind "${kind}".`);
        }
        const open = next.seats.filter((seat) => seat.kind === null);
        if (!open.length) {
          throw new Refused("No seat is open.");
        }
        for (const seat of fill ? open : open.slice(0, 1)) {
          seat.kind = kind as SeatRequest["kind"];
        }
      });
    },
    removeSeat(playerId) {
      apply((next) => {
        const place = placeOf(playerId);
        if (place < 0 || next.seats[place].kind === null || next.seats[place].kind === "human") {
          throw new Refused("This seat cannot be removed.");
        }
        next.seats[place] = { kind: null, faction: null };
      });
    },
    setSettings(settings: TableSettings) {
      apply((next, known) => {
        if (settings.startFrom !== undefined) {
          const scenario = known.scenarios.find((item) => item.id === settings.startFrom);
          if (settings.startFrom !== null && !scenario) {
            throw new Refused(`The engine has no scenario "${settings.startFrom}".`);
          }
          next.startFrom = scenario?.id ?? null;
          // A scenario is a game of its own: the table has its seats.
          if (scenario) {
            resize(next, scenario.players);
          }
        }
        if (settings.players !== undefined && next.startFrom === null) {
          const count = settings.players;
          if (!Number.isInteger(count) || count < known.min_players || count > known.max_players) {
            throw new Refused(
              `A table has ${known.min_players} to ${known.max_players} seats, not ${count}.`,
            );
          }
          resize(next, count);
        }
        if (settings.cards !== undefined && next.startFrom === null) {
          next.cards = settings.cards;
        }
        if (settings.waitMs !== undefined) {
          next.waitMs = Math.min(known.max_bot_delay_ms, Math.max(0, Math.round(settings.waitMs)));
        }
      });
    },
    shuffleSeats() {
      apply((next) => {
        // The seats in another order, if the table has one: seats of the same kind with no
        // faction look the same.
        const look = (seats: Chosen["seats"]) =>
          seats.map((seat) => `${seat.kind}:${seat.faction}`).join();
        const before = look(next.seats);
        if (new Set(next.seats.map((seat) => `${seat.kind}:${seat.faction}`)).size < 2) {
          return;
        }
        while (look(next.seats) === before) {
          for (let place = next.seats.length - 1; place > 0; place--) {
            const other = Math.floor(Math.random() * (place + 1));
            [next.seats[place], next.seats[other]] = [next.seats[other], next.seats[place]];
          }
        }
      });
    },
    rename() {},
    setFaction(playerId, faction) {
      apply((next, known) => {
        const place = placeOf(playerId);
        if (place < 0 || next.seats[place].kind === null) {
          throw new Refused("This seat has no player.");
        }
        const holder = faction
          ? next.seats.findIndex((seat, other) => other !== place && seat.faction === faction)
          : -1;
        if (holder >= 0) {
          const name =
            known.faction_groups
              .flatMap((group) => group.factions)
              .find((item) => item.id === faction)?.name ?? faction;
          throw new Refused(
            `${name} is already taken by ${latest?.lobby.slots[holder]?.nickname ?? "another seat"}`,
          );
        }
        next.seats[place].faction = faction;
      });
    },
    async maps(players): Promise<MapTemplateSummary[]> {
      return (await optionsOf(players)).maps;
    },
    setMap(map) {
      apply((next, known) => {
        if (next.startFrom !== null) {
          throw new Refused("The table starts from a scenario, which has its own map.");
        }
        if (map.kind === "template" && !known.maps.some((item) => item.alias === map.alias)) {
          throw new Refused(`No map "${map.alias}" builds for ${next.seats.length} players.`);
        }
        next.map = map;
      });
    },
    start() {
      queue = queue.then(async () => {
        if (closed || running || !request || !latest) {
          return;
        }
        if (chosen.seats.some((seat) => seat.kind === null)) {
          // The words of the server for the same refusal.
          return say({ kind: "rejected", reason: "Every seat must be filled before starting" });
        }
        const file = waitingFile();
        if (file) {
          return say({ kind: "rejected", reason: `${file.label} is still loading` });
        }
        try {
          // The engine draws the factions as it will for the game: a table that cannot start is
          // refused here, with the reason.
          await setupDrawOf(module, request);
        } catch (error) {
          return say({
            kind: "rejected",
            reason: error instanceof Error ? error.message : String(error),
          });
        }
        running = true;
        latest = { ...latest, lobby: { ...latest.lobby, phase: "running" } };
        say(latest);
        play.onStart({
          seed: play.seed,
          players: chosen.seats.length,
          humans: chosen.seats.reduce(
            (mask, seat, place) => (seat.kind === "human" ? mask | (1 << place) : mask),
            0,
          ),
          table: request,
          ...(chosen.waitMs ? { waitMs: chosen.waitMs } : {}),
        });
      });
    },
    close() {
      closed = true;
      listeners.clear();
    },
  };
}

export interface LocalGameTablePlay {
  /** The wait of the seats that the page plays, as the game has it now. */
  waitMs: number;
  /** The host set another wait: the game is told, and the saved game keeps it. */
  onWait(ms: number): void;
}

/**
 * The table of a game of single player that runs: who sits in each seat, and how the game was
 * set up. It is read from the table that the game was started from
 * (`LocalGame.table`). Of its setup only the wait changes.
 */
export function localGameTable(
  module: WebAssembly.Module,
  game: LocalGame,
  play: LocalGameTablePlay,
): Table {
  const listeners = new Set<(event: TableEvent) => void>();
  const request = game.table;
  let waitMs = play.waitMs;
  let latest: Extract<TableEvent, { kind: "table" }> | null = null;
  let failed: string | null = null;
  let closed = false;
  let version = 1;
  let longest = 30_000;

  const say = (event: TableEvent) => {
    for (const listener of listeners) {
      listener(event);
    }
  };
  const refuse = (reason: string) => say({ kind: "rejected", reason });
  const RUNNING = "The game runs: its table is as it was set up.";

  const read = async () => {
    if (!request) {
      throw new Error("This game was not started from a table.");
    }
    const [known, preview, draw] = await Promise.all([
      setupOptionsOf(module, request.setup.player_count),
      setupPreviewOf(module, request),
      setupDrawOf(module, request),
    ]);
    longest = known.max_bot_delay_ms;
    const scenario = request.setup.start_from ?? null;
    const usual = known.strategy_card_sets.find((set) => set.default)?.id ?? "";
    const kinds = new Map(known.bot_kinds.map((kind) => [kind.id, kind.name]));
    const lobby: LobbyState = {
      game_id: "single",
      phase: "running",
      host_player_id: viewerOf(request.seats),
      // The slots of a running table are the seats of the game, in their order.
      slots: draw.players.map((player, seat) => {
        const joined = request.seats[seat];
        const bot = joined && joined.kind !== "human" ? joined.kind : null;
        return {
          slot_id: `slot_${seat + 1}`,
          position: seat + 1,
          occupant: player,
          // A table of an older page has no names: a seat is named by who plays it.
          nickname: joined?.name ?? (bot ? (kinds.get(bot) ?? bot) : "Player"),
          connected: true,
          can_take_over: false,
          ...(bot ? { bot } : {}),
          ...(draw.factions[seat] ? { faction: draw.factions[seat] } : {}),
        };
      }),
      lobby_version: version,
      bot_kinds: known.bot_kinds.map((kind) => kind.id),
      map: preview.map,
      map_revision: 0,
      strategy_card_set: scenario ? usual : (request.setup.strategy_cards ?? usual),
      speaker_order: request.setup.speaker_order ?? "random",
      factions: known.faction_groups.flatMap((group) => group.factions.map((item) => item.id)),
      bot_delay_ms: waitMs,
      ...(scenario ? { start_from: scenario } : {}),
    };
    return { lobby, preview, known };
  };

  read().then(
    ({ lobby, preview, known }) => {
      if (closed) {
        return;
      }
      latest = {
        kind: "table",
        lobby,
        viewer: lobby.host_player_id,
        preview,
        options: known,
        local: true,
      };
      say(latest);
    },
    (error: unknown) => {
      failed = error instanceof Error ? error.message : String(error);
      say({ kind: "error", message: failed });
    },
  );

  return {
    subscribe(listener) {
      listeners.add(listener);
      if (failed !== null) {
        listener({ kind: "error", message: failed });
      } else if (latest) {
        listener(latest);
      }
      return () => listeners.delete(listener);
    },
    async join() {
      refuse("A table in the page has one person.");
      return null;
    },
    async takeOver() {
      refuse("A table in the page has one person.");
      return null;
    },
    leave() {},
    addBots: () => refuse(RUNNING),
    removeSeat: () => refuse(RUNNING),
    shuffleSeats: () => refuse(RUNNING),
    setSettings(settings: TableSettings) {
      if (settings.waitMs === undefined || !latest) {
        return refuse(RUNNING);
      }
      waitMs = Math.min(longest, Math.max(0, Math.round(settings.waitMs)));
      version += 1;
      play.onWait(waitMs);
      latest = {
        ...latest,
        lobby: { ...latest.lobby, bot_delay_ms: waitMs, lobby_version: version },
      };
      say(latest);
    },
    rename() {},
    setFaction: () => refuse(RUNNING),
    async maps(players): Promise<MapTemplateSummary[]> {
      return (await setupOptionsOf(module, players)).maps;
    },
    setMap: () => refuse(RUNNING),
    start() {},
    close() {
      closed = true;
      listeners.clear();
    },
  };
}
