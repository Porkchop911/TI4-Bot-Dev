// The save store of the page: the single player games of this browser, in the store of
// `web-shared/src/storage/` (IndexedDB `ti4-offline`, one list with web/). The page opens it once.
import {
  type Autosave,
  type EngineIdentity,
  type GameMeta,
  type OfflineSaveStore,
  type SavedGameRecord,
  type SavedGameSummary,
  StoreError,
  type StoreStatus,
  EXPORT_KIND,
  EXPORT_VERSION,
  createAutosave,
  openOfflineSaveStore,
} from "@ti4/shared/storage/index.ts";
import {
  type SaveStorage,
  clearLeft,
  forgetSide,
  leftIds,
  readLeft,
  savedAddress,
} from "./savedGame";
import { humanizeId } from "./select/names";
import type { HostSave } from "./wasmTransport";
import type { SessionUpdate } from "./wire";

export type {
  Autosave,
  GameMeta,
  OfflineSaveStore,
  SavedGameRecord,
  SavedGameSummary,
  StoreStatus,
};

let opened: Promise<OfflineSaveStore> | null = null;

/** The store of this page. Without IndexedDB it keeps the games in memory (`status().warningKind`). */
export const saveStore = (): Promise<OfflineSaveStore> => {
  opened ??= openOfflineSaveStore();
  return opened;
};

/** Debounced writes of the saves of a game, flushed when the page is hidden or left. */
export const autosaveOf = (store: OfflineSaveStore, onError?: () => void): Autosave =>
  createAutosave(store, { onError });

/** A new id for a game. */
export const newSaveId = (): string => crypto.randomUUID();

/** The engine of a save: its build and content from the save, the hash of the file of the page. */
export const engineOfSave = (save: HostSave, wasmSha256: string): EngineIdentity => {
  const engine = (save.engine ?? {}) as { build?: unknown; content_digest?: unknown };
  return {
    build: typeof engine.build === "string" ? engine.build : "",
    contentDigest: typeof engine.content_digest === "string" ? engine.content_digest : "",
    wasmSha256,
  };
};

/** The faction of each seat as an update shows it (bit 0, seat `a`, is the first). */
export const factionsOf = (update: SessionUpdate | null, players: number): Array<string | null> =>
  Array.from(
    { length: players },
    (_, place) =>
      update?.view.players.find((player) => player.id === String.fromCharCode(97 + place))
        ?.faction ?? null,
  );

/**
 * Puts into the store the saves that a page kept in localStorage when it was left, before the
 * store had them (`LeftSave`). A record that was written after the page was left is newer, and
 * stays. Call it before the games are read.
 */
export async function takeLeft(
  store: OfflineSaveStore,
  storage: SaveStorage & Pick<Storage, "length" | "key">,
  nickname: string,
) {
  for (const id of leftIds(storage)) {
    const left = readLeft(storage, id);
    const record = left && (await store.load(id));
    if (left && (!record || record.updatedAt <= left.at)) {
      const { save } = left;
      try {
        await store.put(id, save, {
          nickname: record?.nickname ?? nickname,
          factions:
            left.factions?.length === save.players
              ? left.factions
              : (record?.summary.factions ?? Array.from({ length: save.players }, () => null)),
          engine: engineOfSave(save, left.engine),
          ...(left.slot === undefined ? {} : { slot: left.slot }),
        });
      } catch {
        // A save that the store does not take is not kept.
      }
    }
    clearLeft(storage, id);
  }
}

/** Removes a game, and what localStorage keeps next to it. */
export async function removeSave(store: OfflineSaveStore, storage: SaveStorage, id: string) {
  await store.remove(id);
  forgetSide(storage, id);
}

/** Why the store keeps the games of this page in memory only, in words; null when it does not. */
export function storeWarning(status: StoreStatus): string | null {
  switch (status.warningKind) {
    case "unavailable":
      return "This browser does not keep saved games here (a private window?): they are lost when the page is closed.";
    case "newer-version":
      return "The saved games of this browser were written by a newer version: this page keeps its games until it is closed.";
    case "quota":
      return "The browser has no room for more saved games: the last game is kept until the page is closed.";
    case "write-failed":
      return "The browser did not write the last save: it is kept until the page is closed.";
    default:
      return null;
  }
}

/** Why a file was not imported, in words. */
export function importFailure(error: unknown, text: string): string {
  if (isOlder(text)) {
    return "The file is a saved game of an older version. It cannot be played again.";
  }
  if (error instanceof StoreError) {
    switch (error.code) {
      case "too-large":
        return "The file is too large for a saved game.";
      case "invalid":
        return "The file is not a saved game.";
      case "too-many-games":
      case "quota":
        return "There is no room for another saved game. Delete one on the start screen.";
      default:
        return "The game could not be kept.";
    }
  }
  return "The file could not be read.";
}

/** A file of an older export, or a game of an older page of this app (formats 1 to 5). */
function isOlder(text: string): boolean {
  try {
    const value: unknown = JSON.parse(text);
    if (!value || typeof value !== "object") {
      return false;
    }
    const { kind, version, format } = value as Record<string, unknown>;
    return kind === EXPORT_KIND
      ? typeof version === "number" && version < EXPORT_VERSION
      : typeof format === "number" && format >= 1 && format <= 5;
  } catch {
    return false;
  }
}

/** One game of the list of the start screen. */
export interface SaveRow {
  id: string;
  /** The address that opens it. */
  href: string;
  /** Which kind of game: "Single player", "Local game · seed 3", "Scenario late-game", "Imported game". */
  title: string;
  /** Where the game is, and who plays it here. */
  detail: string;
  /** When it was saved last. */
  updated: string;
}

const PHASES: Record<string, string> = {
  strategy: "strategy phase",
  action: "action phase",
  status: "status phase",
  agenda: "agenda phase",
};

/** The rows of the saved games, newest first. */
export function saveRows(games: readonly SavedGameSummary[], now = Date.now()): SaveRow[] {
  return games.map((game) => {
    const { summary } = game;
    const slot = game.slot ?? "";
    const title =
      slot === "single"
        ? "Single player"
        : slot.startsWith("local.")
          ? `Local game · seed ${summary.seed}`
          : slot.startsWith("scenario.")
            ? `Scenario ${slot.slice("scenario.".length, slot.lastIndexOf("."))}`
            : game.imported
              ? "Imported game"
              : "Saved game";
    const factions = summary.factions
      .filter((faction, seat) => faction !== null && (summary.humans >> seat) & 1)
      .map((faction) => humanizeId(faction as string));
    const phase = summary.phase === null ? null : (PHASES[summary.phase] ?? summary.phase);
    const where =
      game.status === "finished"
        ? "Finished"
        : summary.round === null
          ? "Not started"
          : `Round ${summary.round}${phase ? ` · ${phase}` : ""}`;
    return {
      id: game.id,
      href: `/${savedAddress(game)}`,
      title,
      detail: [
        where,
        `${summary.playerCount} seats`,
        ...(factions.length ? [factions.join(", ")] : []),
        `${summary.answers} answers`,
      ].join(" · "),
      updated: ago(now - game.updatedAt),
    };
  });
}

const ago = (ms: number) => {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) {
    return "just now";
  }
  if (minutes < 60) {
    return `${minutes} min ago`;
  }
  const hours = Math.floor(minutes / 60);
  return hours < 48 ? `${hours} h ago` : `${Math.floor(hours / 24)} days ago`;
};
