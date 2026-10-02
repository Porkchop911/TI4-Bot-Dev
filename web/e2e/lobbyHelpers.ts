import { expect, type APIRequestContext, type Page } from "@playwright/test";
import type { InitialSnapshotMsg } from "../src/protocol/types";

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;

export async function gameSnapshot(
  request: APIRequestContext,
  gameId: string,
  session: string,
): Promise<InitialSnapshotMsg> {
  const response = await request.get(`${backend}/api/games/${gameId}/snapshot`, {
    headers: { "x-ti4-player-session": session },
  });
  expect(response.ok(), `snapshot for game ${gameId}: ${response.status()}`).toBe(true);
  return response.json();
}

export async function createStartedGame(
  request: APIRequestContext,
  playerCount: number,
  seed: number,
) {
  const created = await request.post(`${backend}/api/games`, {
    data: { player_count: playerCount, seed, nickname: "E2E Host" },
  });
  expect(created.ok()).toBeTruthy();
  const host = await created.json();
  const gameId: string = host.game_id;
  const players: { id: string; session: string }[] = [
    { id: host.player.id, session: host.player_session },
  ];
  for (let i = 1; i < playerCount; i++) {
    const joined = await request.post(`${backend}/api/games/${gameId}/lobby/join`, {
      data: { kind: "new", nickname: `E2E Player ${i + 1}` },
    });
    expect(joined.ok()).toBeTruthy();
    const result = await joined.json();
    players.push({ id: result.player.id, session: result.player_session });
  }
  for (const player of players) {
    const ready = await request.post(`${backend}/api/games/${gameId}/lobby/ready`, {
      data: { ready: true },
      headers: { "x-ti4-player-session": player.session },
    });
    expect(ready.ok()).toBeTruthy();
  }
  const started = await request.post(`${backend}/api/games/${gameId}/lobby/start`, {
    headers: { "x-ti4-player-session": players[0].session },
  });
  expect(started.ok()).toBeTruthy();
  return { gameId, players };
}

export async function openPlayerGame(page: Page, gameId: string, session: string) {
  await page.goto(`/games/${gameId}`);
  await page.evaluate(
    ({ gameId, session }) => sessionStorage.setItem(`ti4.player-session:${gameId}`, session),
    { gameId, session },
  );
  await page.reload();
}
