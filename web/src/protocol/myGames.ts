/**
 * The games this browser created or joined, kept in localStorage so they survive a closed tab:
 * the seat token is what lets the owner open the game again or end it.
 */
const KEY = "ti4.my-games";

export interface MyGame {
  gameId: string;
  token?: string;
  playerId?: string;
  nickname?: string;
  /** Unix seconds when this browser first stored the game. */
  storedAt: number;
}

function read(): Record<string, MyGame> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return parsed && typeof parsed === "object" ? (parsed as Record<string, MyGame>) : {};
  } catch {
    return {};
  }
}

function write(games: Record<string, MyGame>): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(games));
  } catch {
    // Storage may be blocked; the game still works from the tab.
  }
}

export function rememberMyGame(game: Omit<MyGame, "storedAt"> & { storedAt?: number }): void {
  const all = read();
  const previous = all[game.gameId];
  all[game.gameId] = {
    ...previous,
    ...game,
    storedAt: previous?.storedAt ?? game.storedAt ?? Math.floor(Date.now() / 1000),
  };
  write(all);
}

export function forgetMyGame(gameId: string): void {
  const all = read();
  delete all[gameId];
  write(all);
  try {
    sessionStorage.removeItem(`ti4.player-session:${gameId}`);
  } catch {
    // ignore
  }
}

export function myGame(gameId: string): MyGame | undefined {
  return read()[gameId];
}

/** The seat token this browser holds for a game (this tab first, then the saved list). */
export function tokenFor(gameId: string): string | undefined {
  try {
    const tab = sessionStorage.getItem(`ti4.player-session:${gameId}`);
    if (tab) return tab;
  } catch {
    // ignore
  }
  return read()[gameId]?.token;
}

export function listMyGames(): MyGame[] {
  return Object.values(read()).sort((a, b) => b.storedAt - a.storedAt);
}

export function ageLabel(unixSeconds: number | null | undefined, nowMs = Date.now()): string | null {
  if (!unixSeconds) return null;
  const minutes = Math.max(0, Math.floor((nowMs / 1000 - unixSeconds) / 60));
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} d ago`;
}
