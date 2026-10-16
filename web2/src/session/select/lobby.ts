// The lobby of a table: the rows of the players with their signs, the setup, and the footer.
// The components decide nothing from names: what a row shows and which controls it has is here.
import type {
  FactionSheetView,
  LobbyButtonView,
  LobbyFactionEditorView,
  LobbyFactionOptionView,
  LobbyFooterView,
  LobbyIntent,
  LobbyRowView,
  LobbySettingsChange,
  LobbySetupRowView,
  LobbySignView,
  LobbyStepView,
  LobbyView,
} from "../../model";
import type { TableState } from "../table";
import type { Catalog, LobbySlot, LobbyState, SetupOptions } from "../wire";
import { SEAT_PLACE, selectPreviewBoard } from "./board";
import { factionSheet } from "./factionSheet";
import { factionName, titleCase } from "./names";

/** What the page knows of the table besides the table itself. */
export interface LobbyLocal {
  /** The players that this browser has at the table, by id. */
  mine: string[];
  inspected: string | null;
  offline: boolean;
}

/**
 * The sign of each kind of seat that the server plays in its own process. A kind with no sign
 * is not offered. The names are those of the setup options of the server.
 */
const KIND_SIGNS: Record<string, LobbySignView["kind"]> = { smart: "rule", random: "random" };
/** The kind of "Fill with …": the usual choice. */
const USUAL_KIND = "smart";
/** One step of the wait of the seats that the server plays, in milliseconds. */
const WAIT_STEP = 500;

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;

/** The kinds that a host can add, as the server names them. Empty until it said the names. */
const kindsOf = (lobby: LobbyState, options: SetupOptions | null) =>
  (options?.bot_kinds ?? []).filter(
    (kind) => kind.id in KIND_SIGNS && lobby.bot_kinds.includes(kind.id),
  );

/** Who plays the seats that no person has: the server, or for a table in the page, the page. */
const playerOf = (local: boolean) => (local ? "the page" : "the server");

function sign(
  slot: LobbySlot,
  lobby: LobbyState,
  you: boolean,
  options: SetupOptions | null,
  local: boolean,
): LobbySignView | null {
  if (slot.occupant === null) {
    return { kind: "open", word: "Open seat" };
  }
  if (slot.bot) {
    const name = options?.bot_kinds.find((kind) => kind.id === slot.bot)?.name;
    return {
      kind: KIND_SIGNS[slot.bot] ?? "random",
      word: name ? `${name}: played by ${playerOf(local)}` : `Played by ${playerOf(local)}`,
    };
  }
  const host = slot.occupant === lobby.host_player_id;
  // The page shows its own table: its player is here, whatever the server said last.
  if (!slot.connected && !you) {
    return { kind: "away", word: host ? "Host · not connected" : "Not connected" };
  }
  return host ? { kind: "host", word: "Host" } : null;
}

function mapName(lobby: LobbyState): string {
  const { map } = lobby;
  const name = map.kind === "random" ? "Random" : (map.name ?? titleCase(map.alias ?? "Map"));
  return `${name} · ${plural(map.systems, "system")}${map.hyperlanes ? " · hyperlanes" : ""}`;
}

/** The name of a new player for another tab of the viewer: "Ada 2", "Ada 3". */
export function nextNickname(lobby: LobbyState, viewer: string): string {
  const taken = new Set(lobby.slots.map((slot) => slot.nickname));
  const own = lobby.slots.find((slot) => slot.occupant === viewer)?.nickname ?? "Player";
  const base = own.replace(/ \d+$/, "");
  for (let number = 2; ; number++) {
    // A nickname has 64 bytes at most: a long name gives room to its number.
    const name = `${base.slice(0, 56)} ${number}`;
    if (!taken.has(name)) {
      return name;
    }
  }
}

/** "Off", "0.5 s", "1 s". */
const waitText = (ms: number) => (ms === 0 ? "Off" : `${ms / 1000} s`);

function setupRows(
  lobby: LobbyState,
  options: SetupOptions | null,
  host: boolean,
  local: boolean,
): LobbySetupRowView[] {
  const seats = lobby.slots.length;
  const set = (change: LobbySettingsChange): LobbyIntent => ({ type: "set", change });
  const step = (label: string, reason: string, change: LobbySettingsChange): LobbyStepView => ({
    label,
    intent: reason ? null : set(change),
    reason,
  });
  // A scenario is a game of its own: its seats, its map and its cards are not chosen. After
  // the start nothing of the setup is chosen but the wait.
  const scenario = lobby.start_from ?? null;
  const before = lobby.phase === "lobby";
  const free = host && scenario === null && before;
  const min = options?.min_players ?? 2;
  const max = options?.max_players ?? 8;
  const people = lobby.slots.filter((slot) => slot.occupant !== null && !slot.bot).length;

  const sets = options?.strategy_card_sets ?? [];
  const usual = sets.find((cards) => cards.default);
  const wait = lobby.bot_delay_ms;
  const longest = options?.max_bot_delay_ms ?? 30_000;
  const scenarios = options?.scenarios ?? [];

  const rows: LobbySetupRowView[] = [
    {
      id: "players",
      label: "Players",
      value: String(seats),
      control: free
        ? {
            kind: "counter",
            less: step(
              "One player fewer",
              seats <= min
                ? `A table has ${min} seats at least`
                : people >= seats
                  ? "A person is in every seat"
                  : "",
              { players: seats - 1 },
            ),
            more: step("One player more", seats >= max ? `A table has ${max} seats at most` : "", {
              players: seats + 1,
            }),
          }
        : null,
      help: [],
    },
    { id: "map", label: "Map", value: mapName(lobby), control: null, help: [] },
    {
      id: "cards",
      label: "Strategy cards",
      value:
        sets.find((cards) => cards.id === lobby.strategy_card_set)?.name ??
        titleCase(lobby.strategy_card_set),
      control:
        free && sets.length
          ? {
              kind: "list",
              chosen: lobby.strategy_card_set,
              options: sets.map((cards) => ({
                id: cards.id,
                label: cards.name,
                intent: set({ cards: cards.id }),
              })),
            }
          : null,
      // What a set changes is its cards that the usual set does not have.
      help: sets
        .filter((cards) => cards !== usual)
        .map((cards) => ({
          title: cards.name,
          text: cards.cards
            .filter((card) => !usual?.cards.some((other) => other.id === card.id))
            .map((card) => `${card.initiative} ${card.name}`)
            .join(", "),
        }))
        .filter((item) => item.text),
    },
    {
      id: "wait",
      label: "Bots wait",
      value: waitText(wait),
      control: host
        ? {
            kind: "counter",
            less: step("Wait less", wait <= 0 ? "The seats answer at once" : "", {
              waitMs: Math.max(0, (Math.ceil(wait / WAIT_STEP) - 1) * WAIT_STEP),
            }),
            more: step("Wait more", wait >= longest ? `${waitText(longest)} at most` : "", {
              waitMs: Math.min(longest, (Math.floor(wait / WAIT_STEP) + 1) * WAIT_STEP),
            }),
          }
        : null,
      help: [
        {
          title: "Bots wait",
          text: `A seat that ${playerOf(local)} plays waits about this long before each answer. It does not wait to decline a reaction window.`,
        },
      ],
    },
  ];
  // The public server starts no scenario: the row is there when there is one to say or choose.
  if (scenario !== null || scenarios.length) {
    rows.push({
      id: "start",
      label: "Start from",
      value:
        scenario === null
          ? "Round 1"
          : (scenarios.find((item) => item.id === scenario)?.title ??
            titleCase(scenario.replaceAll("-", " "))),
      control:
        host && before && scenarios.length
          ? {
              kind: "list",
              chosen: scenario ?? "",
              options: [
                { id: "", label: "Round 1", intent: set({ startFrom: null }) },
                ...scenarios.map((item) => ({
                  id: item.id,
                  label: item.title,
                  intent: set({ startFrom: item.id }),
                })),
              ],
            }
          : null,
      help: scenarios.map((item) => ({ title: item.title, text: item.description })),
    });
  }
  return rows;
}

/**
 * What the host changed between two views of a table, for a player who is not the host. Null:
 * nothing of the setup.
 */
export function hostChange(before: LobbyState, after: LobbyState): string | null {
  if (before.start_from !== after.start_from) {
    return "The host changed where the game starts";
  }
  if (before.slots.length !== after.slots.length) {
    return `The host set the players to ${after.slots.length}`;
  }
  if (before.strategy_card_set !== after.strategy_card_set) {
    return "The host changed the strategy cards";
  }
  if (before.map_revision !== after.map_revision) {
    return "The host changed the map";
  }
  if (before.bot_delay_ms !== after.bot_delay_ms) {
    return `The host set the wait of the bots to ${waitText(after.bot_delay_ms).toLowerCase()}`;
  }
  return null;
}

/**
 * The factions that a seat can be given, in their groups. The groups are those of the setup
 * options; until the server said them, the factions of the table are one list. A faction has the
 * name that the rows call it by, from the catalogue ("Hacan"), else the name of the server.
 */
function factionGroups(
  lobby: LobbyState,
  options: SetupOptions | null,
  catalog: Catalog | null,
): { name: string; factions: { id: string; name: string }[] }[] {
  if (options?.faction_groups.length) {
    return options.faction_groups.map((group) => ({
      name: group.name,
      factions: group.factions.map((faction) => ({
        id: faction.id,
        name: catalog?.components.faction_sheets?.[faction.id]?.name ?? faction.name,
      })),
    }));
  }
  return [
    { name: "", factions: lobby.factions.map((id) => ({ id, name: factionName(id, catalog) })) },
  ];
}

export function selectLobby(
  table: TableState,
  local: LobbyLocal,
  catalog: Catalog | null,
): LobbyView {
  const { lobby, viewer, options } = table;
  const single = table.local === true;
  const host = viewer !== null && viewer === lobby.host_player_id;
  const seats = lobby.slots.length;
  const open = lobby.slots.filter((slot) => slot.occupant === null).length;
  const kinds = kindsOf(lobby, options);
  const hostName =
    lobby.slots.find((slot) => slot.occupant === lobby.host_player_id)?.nickname ?? "the host";

  const usual = kinds.find((kind) => kind.id === USUAL_KIND);

  const sheets = new Map<string, FactionSheetView | null>();
  const sheetOf = (faction: string) => {
    if (!sheets.has(faction)) {
      sheets.set(faction, factionSheet(faction, catalog));
    }
    return sheets.get(faction) ?? null;
  };
  const groups = factionGroups(lobby, options, catalog);
  /** The choice of the faction of a seat. A faction of another seat stays, and says whose it is. */
  const editor = (slot: LobbySlot): LobbyFactionEditorView => {
    const player = slot.occupant!;
    const option = (id: string, name: string): LobbyFactionOptionView => {
      const chosen = (slot.faction ?? "") === id;
      const holder = id
        ? lobby.slots.find((other) => other !== slot && other.faction === id)
        : null;
      return {
        id,
        name,
        chosen,
        intent: chosen || holder ? null : { type: "faction", player, faction: id || null },
        taken: holder?.nickname ?? "",
        sheet: id ? sheetOf(id) : null,
      };
    };
    return {
      label: `Faction of ${slot.nickname}`,
      random: option("", "Random"),
      groups: groups.map((group) => ({
        name: group.name,
        options: group.factions.map((faction) => option(faction.id, faction.name)),
      })),
    };
  };
  // The seats of a scenario have its factions, and after the start nothing is chosen.
  const before = lobby.phase === "lobby";
  const choosing = before && (lobby.start_from ?? null) === null;

  let firstOpen = true;
  const rows = lobby.slots.map((slot): LobbyRowView => {
    const you = slot.occupant !== null && slot.occupant === viewer;
    const buttons: LobbyButtonView[] = [];
    if (slot.occupant === null) {
      if (host) {
        for (const kind of kinds) {
          buttons.push({ label: kind.name, intent: { type: "addBot", kind: kind.id } });
        }
      }
      // One row has it: every open seat is the same seat until the game starts.
      if (viewer !== null && firstOpen && !single) {
        buttons.push({
          label: "Join in a new tab",
          title: "Take this seat as another player, in a new tab of this browser",
          intent: { type: "joinTab" },
        });
      }
      firstOpen = false;
    }
    // A person who is away from a running game: a visitor can sit down in the seat.
    if (!before && viewer === null && slot.occupant !== null && slot.can_take_over) {
      const name = slot.nickname ?? "Player";
      // The row has the name: the button does not repeat it (a name can be long).
      buttons.push({
        label: "Rejoin",
        title: `Rejoin as ${name}: play this seat from this tab`,
        intent: { type: "takeOver", player: slot.occupant, name },
      });
    }
    const removable = before && host && slot.occupant !== null && !you;
    return {
      id: slot.slot_id,
      player: slot.occupant,
      sign: sign(slot, lobby, you, options, single),
      name: slot.nickname ?? "Open",
      you,
      // The player of a table in the page has the name of this browser: the start screen has it.
      rename: you && !single && before,
      become:
        slot.occupant !== null && !you && local.mine.includes(slot.occupant)
          ? { type: "become", player: slot.occupant }
          : null,
      faction:
        slot.occupant === null ? "" : slot.faction ? factionName(slot.faction, catalog) : "Random",
      sheet: slot.occupant !== null && slot.faction ? sheetOf(slot.faction) : null,
      // A row of another person is not a control, also for the host.
      editor:
        choosing && slot.occupant !== null && (you || (host && !!slot.bot)) ? editor(slot) : null,
      buttons,
      remove: removable
        ? {
            label: `Remove ${slot.nickname}`,
            intent: { type: "remove", player: slot.occupant! },
          }
        : null,
    };
  });

  const blocked = open ? `Waiting for ${plural(open, "open seat")}` : "";
  const footer: LobbyFooterView = !before
    ? {
        // A game that runs waits for nobody here. A visitor opens it, with no seat.
        note: "",
        main:
          viewer === null ? { label: "Watch", disabled: false, intent: { type: "watch" } } : null,
        nickname: false,
      }
    : viewer === null
      ? {
          note: open ? plural(open, "open seat") : "Every seat is taken",
          main: { label: "Join", disabled: open === 0, intent: { type: "join" } },
          nickname: open > 0,
        }
      : host
        ? {
            note: blocked,
            main: { label: "Start game", disabled: blocked !== "", intent: { type: "start" } },
            nickname: false,
          }
        : { note: blocked || `Waiting for ${hostName} to start`, main: null, nickname: false };

  const preview = table.preview;
  return {
    title: `${single ? "Single player" : before ? "Lobby" : "Table"} · ${plural(seats, "player")}`,
    status: local.offline ? "Offline · reconnecting" : "",
    board: selectPreviewBoard(
      preview?.tiles ?? [],
      preview?.seats.map((seat) => seat.home_system_id) ?? [],
      local.inspected,
    ),
    seats: {
      [SEAT_PLACE]: { id: SEAT_PLACE, name: "", faction: "Seat", symbol: "●", color: "#8a94a6" },
    },
    setup: setupRows(lobby, options, host, single),
    filled: `${seats - open} of ${seats}`,
    help: kinds.map((kind) => ({ title: kind.name, text: kind.description })),
    rows,
    fill:
      host && open > 0 && usual
        ? {
            label: `Fill with ${usual.name.toLowerCase()}s`,
            intent: { type: "fillBots", kind: usual.id },
          }
        : null,
    footer,
  };
}
