import type { Page } from "@playwright/test";
import type { LobbyDto } from "../../../src/protocol/types";
import { GAME_ID, SESSION } from "./mockGame";

/** A lobby with the host (the viewer), one other player, and two free seats. */
export const lobbyFixture: LobbyDto = {
  game_id: GAME_ID,
  phase: "lobby",
  lobby_version: 3,
  host_player_id: "host_seat",
  slots: [
    { slot_id: "slot-1", position: 1, occupant: "host_seat", nickname: "Alex", ready: true, connected: true, can_take_over: false },
    { slot_id: "slot-2", position: 2, occupant: "guest_seat", nickname: "Blair", ready: false, connected: true, can_take_over: false },
    { slot_id: "slot-3", position: 3, occupant: null, nickname: null, ready: false, connected: false, can_take_over: false },
    { slot_id: "slot-4", position: 4, occupant: null, nickname: null, ready: false, connected: false, can_take_over: false },
  ],
};

/** Renders the real lobby at /games/<id> as its host, against routed HTTP (no backend). */
export async function openMockedLobby(page: Page, lobby: LobbyDto = lobbyFixture) {
  await page.route(`**/api/games/${GAME_ID}/lobby/join`, (route) =>
    route.fulfill({ json: { player_session: SESSION, player: { id: lobby.host_player_id }, lobby } }),
  );
  await page.route(`**/api/games/${GAME_ID}/lobby/heartbeat`, (route) => route.fulfill({ json: {} }));
  await page.route(`**/api/games/${GAME_ID}/lobby`, (route) => route.fulfill({ json: lobby }));
  await page.goto("/");
  await page.evaluate(
    ([id, credential]) => sessionStorage.setItem(`ti4.player-session:${id}`, credential),
    [GAME_ID, SESSION],
  );
  await page.goto(`/games/${GAME_ID}`);
}
