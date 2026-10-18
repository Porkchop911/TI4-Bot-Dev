import { expect, type APIRequestContext, type Page } from "@playwright/test";
import { createGameBody } from "./smokePreset";
import type { InitialSnapshotMsg } from "../src/protocol/types";

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;

/**
 * The seat's snapshot. A batch commit makes the server replay the whole game, which can take a few
 * seconds late in a debug-build game (and longer when the disk is busy), so the request waits up to
 * `timeoutMs` and is tried once more before the error reaches the caller.
 */
export async function gameSnapshot(
  request: APIRequestContext,
  gameId: string,
  session: string,
  timeoutMs = 15_000,
): Promise<InitialSnapshotMsg> {
  const get = () =>
    request.get(`${backend}/api/games/${gameId}/snapshot`, {
      headers: { "x-ti4-player-session": session },
      timeout: timeoutMs,
    });
  const response = await get().catch(() => get());
  // Playwright keeps every response body in the worker until it is disposed; the smoke harness
  // polls this several times per decision, which grew a long game's worker to 7 GB (OOM kill).
  try {
    // The body says why (e.g. "Game session failed closed: ..."), which the status alone does not.
    const reason = response.ok() ? "" : ` ${(await response.text().catch(() => "")).slice(0, 400)}`;
    expect(response.ok(), `snapshot for game ${gameId}: ${response.status()}${reason}`).toBe(true);
    return (await response.json()) as InitialSnapshotMsg;
  } finally {
    await response.dispose().catch(() => {});
  }
}

export async function createStartedGame(
  request: APIRequestContext,
  playerCount: number,
  seed: number,
  startPreset?: string,
  cardSet?: string,
  mapTemplate?: string,
  /** Factions the host sets per seat (in join order) before the start; left out: drawn. */
  factions?: (string | undefined)[],
) {
  const created = await request.post(`${backend}/api/games`, {
    // The server draws the opening speaker from the seed by default; these specs drive the host
    // as the first picker, so they pin the first seat. TI4_SMOKE_SPEAKER=random opts back in.
    // The seats are the order of the join, so `players` below is in seat order.
    data: {
      ...createGameBody(playerCount, seed, startPreset, cardSet, mapTemplate),
      speaker_order: process.env.TI4_SMOKE_SPEAKER?.trim() || "seat",
    },
  });
  expect(created.ok()).toBeTruthy();
  const host = await created.json();
  const gameId: string = host.game_id;
  const players: { id: string; session: string }[] = [
    { id: host.player.id, session: host.player_session },
  ];
  // Opt-in (TI4_SMOKE_MAP=1): exercise the host's map choice on part of the runs. The server
  // draws a fresh private seed for every choice, so a run that sets this is not reproducible
  // from `seed` alone.
  if (process.env.TI4_SMOKE_MAP) {
    const listed = await request.get(`${backend}/api/maps?player_count=${playerCount}`);
    expect(listed.ok()).toBeTruthy();
    const templates: { alias: string }[] = await listed.json();
    const map =
      templates.length > 0
        ? { kind: "template", alias: templates[seed % templates.length].alias }
        : { kind: "random" };
    const chosen = await request.post(`${backend}/api/games/${gameId}/lobby/map`, {
      data: { map },
      headers: { "x-ti4-player-session": players[0].session },
    });
    expect(chosen.ok(), `choosing ${JSON.stringify(map)}: ${chosen.status()}`).toBeTruthy();
  }
  for (let i = 1; i < playerCount; i++) {
    const joined = await request.post(`${backend}/api/games/${gameId}/lobby/join`, {
      data: { kind: "new", nickname: `E2E Player ${i + 1}` },
    });
    expect(joined.ok()).toBeTruthy();
    const result = await joined.json();
    players.push({ id: result.player.id, session: result.player_session });
  }
  for (const [seat, faction] of (factions ?? []).entries()) {
    if (!faction) continue;
    const set = await request.post(`${backend}/api/games/${gameId}/lobby/seat-faction`, {
      data: { player_id: players[seat].id, faction },
      headers: { "x-ti4-player-session": players[0].session },
    });
    expect(set.ok(), `seat ${seat} as ${faction}: ${set.status()} ${await set.text()}`).toBeTruthy();
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
