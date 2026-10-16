import type { Page } from "@playwright/test";
import type { BoardView, GameEvent, LobbyDto, PlayerView, SystemView } from "../../../src/protocol/types";
import { galleryBoard, galleryPlayers } from "../_shared/fixtures";
import { GAME_ID, openMockedGame } from "../_shared/mockGame";
import { openMockedLobby } from "../_shared/mockLobby";
import { me, playerWithHand } from "../_shared/players";

/** `before` is taken on the commit with the old Okabe-Ito colours. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

const FACTIONS = ["sol", "hacan", "xxcha", "letnev", "yssaril", "arborec", "norr", "nekro"];
const NAMES = ["Alex", "Blair", "Casey", "Drew", "Emery", "Frankie", "Gray", "Harper"];
export const seatId = (i: number) => (i === 0 ? me.id : i === 1 ? galleryPlayers[1].id : `seat_${i + 1}`);

export function seatedPlayers(n: number): PlayerView[] {
  return Array.from({ length: n }, (_, i) => {
    const base = i === 0 ? playerWithHand() : i === 1 ? galleryPlayers[1] : { ...galleryPlayers[1], id: seatId(i) };
    return { ...base, id: seatId(i), faction: FACTIONS[i], victory_points: (i * 2) % 5 };
  });
}

/** The gallery board with a fleet and a command token of every seat on systems that are visible. */
export function seatedBoard(n: number): BoardView {
  const systems: Record<string, SystemView> = { ...galleryBoard.systems };
  const spots = ["25", "24", "20", "27", "28", "29", "30", "31"];
  for (let i = 2; i < n; i++) {
    const s = systems[spots[i]];
    systems[spots[i]] = {
      ...s,
      units: [...s.units, { unit_type: "carrier", owner: seatId(i), damaged: false }, { unit_type: "fighter", owner: seatId(i), damaged: false }],
    };
  }
  const tokenSystem = systems["18"];
  systems["18"] = { ...tokenSystem, command_tokens: Array.from({ length: n }, (_, i) => seatId(i)) };
  return { ...galleryBoard, systems };
}

export const nicknames = (n: number) => Object.fromEntries(Array.from({ length: n }, (_, i) => [seatId(i), NAMES[i]]));

export function seatedEvents(n: number): GameEvent[] {
  const events: GameEvent[] = [
    { id: "start", timestamp: "10:00", visibility: { visibility: "public" as const }, event: { kind: "game_initialized", round: 2, phase: "action", speaker: seatId(0) } },
  ];
  for (let i = 0; i < n; i++) {
    events.push({
      id: `choice-${i + 1}`,
      timestamp: `10:${String(i + 1).padStart(2, "0")}`,
      version: i + 2,
      visibility: { visibility: "public" as const },
      event: { kind: "decision_resolved" },
      round: 2,
      phase: "action",
      action_id: `action_${i + 1}`,
      action_type: "tactical",
      action_actor: seatId(i),
      actor: seatId(i),
      stage: "movement",
      decision_count: i + 2,
      detail: `Moved a fleet to system ${18 + i}`,
    } as GameEvent);
  }
  return events;
}

export async function openSeated(page: Page, n: number, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    players: seatedPlayers(n),
    board: seatedBoard(n),
    nicknames: nicknames(n),
    events: seatedEvents(n),
    choice: { prompt: "action phase", player: seatId(1), options: [{ id: "pass", label: "pass", kind: "action" }] },
  });
}

export async function openSeatedLobby(page: Page, n: number, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  const lobby: LobbyDto = {
    game_id: GAME_ID,
    phase: "lobby",
    lobby_version: 3,
    host_player_id: seatId(0),
    slots: Array.from({ length: n }, (_, i) => ({
      slot_id: `slot-${i + 1}`,
      position: i + 1,
      occupant: seatId(i),
      nickname: NAMES[i],
      ready: i % 2 === 0,
      connected: true,
      can_take_over: false,
    })),
  };
  await openMockedLobby(page, lobby);
}
