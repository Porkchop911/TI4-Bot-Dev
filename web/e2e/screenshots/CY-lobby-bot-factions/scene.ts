import type { Page } from "@playwright/test";
import type { LobbyDto, LobbySlot } from "../../../src/protocol/types";
import { fixtureMapChoice } from "../../../src/dev/mapPickerFixtures";
import { GAME_ID, SESSION } from "../_shared/mockGame";

const POOL = ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"];

export interface FactionLobbyOptions {
  players?: number;
  viewer?: "host" | "guest_1";
  /** Bots already seated after the guest, each with the faction it was given (`null`: random). */
  bots?: (string | null)[];
  /**
   * A faction the server rejects as taken although this lobby view does not show it (a second
   * host tab took it a moment ago): the add or change fails with the server's 409 text.
   */
  hiddenHolder?: { faction: string; name: string };
}

/**
 * The real lobby at /games/<id> against a routed server that implements the smart-bot route with
 * a `faction`, `POST /lobby/seat-faction` and `remove-bot`, answering duplicates with the 409 text
 * the real server sends. The server is mocked, the components are not.
 */
export async function openFactionLobby(page: Page, options: FactionLobbyOptions = {}) {
  const players = options.players ?? 5;
  const viewer = options.viewer ?? "host";
  const slots: LobbySlot[] = Array.from({ length: players }, (_, index) => ({
    slot_id: `slot_${index + 1}`,
    position: index + 1,
    occupant: null,
    nickname: null,
    ready: false,
    connected: false,
    can_take_over: false,
  }));
  slots[0] = { ...slots[0], occupant: "host", nickname: "Alex", ready: true, connected: true };
  slots[1] = { ...slots[1], occupant: "guest_1", nickname: "Blair", ready: false, connected: true };
  let botNumber = 0;
  const addBot = (faction: string | null) => {
    const index = slots.findIndex((slot) => !slot.occupant);
    if (index < 0) return;
    botNumber += 1;
    slots[index] = {
      ...slots[index],
      occupant: `bot_${botNumber}`,
      nickname: `Rule bot ${botNumber}`,
      ready: true,
      connected: true,
      bot: "smart",
      ...(faction ? { faction } : {}),
    };
  };
  for (const faction of options.bots ?? []) addBot(faction);

  const lobby = (): LobbyDto => ({
    game_id: GAME_ID,
    phase: "lobby",
    lobby_version: 3 + botNumber,
    host_player_id: "host",
    map: fixtureMapChoice(`${players}pStandard`, players),
    map_revision: 1,
    slots: slots.map((slot) => ({ ...slot })),
    bot_service_enabled: false,
    bot_kinds: ["random", "smart"],
    factions: POOL,
  });
  const refusal = (faction: string | undefined, except?: string) => {
    if (!faction) return null;
    if (!POOL.includes(faction))
      return { status: 400, text: `Faction '${faction}' is not available: choose one of ${POOL.join(", ")}` };
    if (options.hiddenHolder?.faction === faction)
      return { status: 409, text: `Faction '${faction}' is already taken by ${options.hiddenHolder.name}` };
    const holder = slots.find((slot) => slot.faction === faction && slot.occupant !== except);
    return holder ? { status: 409, text: `Faction '${faction}' is already taken by ${holder.nickname}` } : null;
  };

  await page.route(`**/api/games/${GAME_ID}/lobby/bots`, async (route) => {
    const body = route.request().postDataJSON() as { fill?: boolean; count?: number; faction?: string };
    const refused = refusal(body.faction);
    if (refused) return route.fulfill({ status: refused.status, body: refused.text });
    const wanted = body.fill ? slots.filter((slot) => !slot.occupant).length : (body.count ?? 1);
    for (let i = 0; i < wanted; i += 1) addBot(body.faction ?? null);
    return route.fulfill({ json: lobby() });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/seat-faction`, async (route) => {
    const body = route.request().postDataJSON() as { player_id: string; faction: string | null };
    const refused = refusal(body.faction ?? undefined, body.player_id);
    if (refused) return route.fulfill({ status: refused.status, body: refused.text });
    const index = slots.findIndex((slot) => slot.occupant === body.player_id);
    if (index >= 0) slots[index] = { ...slots[index], faction: body.faction ?? undefined };
    return route.fulfill({ json: lobby() });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/remove-bot`, async (route) => {
    const { player_id } = route.request().postDataJSON() as { player_id: string };
    const index = slots.findIndex((slot) => slot.occupant === player_id);
    if (index >= 0)
      slots[index] = { ...slots[index], occupant: null, nickname: null, ready: false, connected: false, bot: undefined, faction: undefined };
    return route.fulfill({ json: lobby() });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/join`, (route) =>
    route.fulfill({ json: { player_session: SESSION, player: { id: viewer }, lobby: lobby() } }),
  );
  await page.route(`**/api/games/${GAME_ID}/lobby/heartbeat`, (route) => route.fulfill({ json: {} }));
  await page.route(`**/api/games/${GAME_ID}/lobby`, (route) => route.fulfill({ json: lobby() }));
  await page.goto("/");
  await page.evaluate(
    ([id, credential]) => {
      sessionStorage.setItem(`ti4.player-session:${id}`, credential);
      sessionStorage.setItem(`ti4.map-picker-seen:${id}`, "1");
    },
    [GAME_ID, SESSION],
  );
  await page.goto(`/games/${GAME_ID}`);
}
