import type { Page } from "@playwright/test";
import type { LobbyDto, LobbySlot } from "../../../src/protocol/types";
import { fixtureMapChoice } from "../../../src/dev/mapPickerFixtures";
import { GAME_ID, SESSION } from "../_shared/mockGame";

export interface BotLobbyOptions {
  players?: number;
  /** The viewer: the host (seat 1) or a joined guest (seat 2). */
  viewer?: "host" | "guest_1";
  /** Bot kinds the server reports; `undefined` omits the field, as an older server does. */
  botKinds?: string[];
  /** Bots already seated, after the guest. */
  initialBots?: number;
  /** The kind of the bots already seated. */
  initialKind?: "random" | "smart";
  /** Seat a second human (Blair) before the bots. */
  guest?: boolean;
  /** A server with the MLP bot service configured as well. */
  mlp?: boolean;
}

const HUMANS = ["Alex", "Blair"];

/**
 * The real lobby at /games/<id> against a routed server that implements the bots route with a
 * `kind` (`random` or `smart`): `POST /lobby/bots` seats one bot (`count`) or fills every open
 * seat (`fill`), and
 * `POST /lobby/remove-bot` frees a seat. The server is mocked, the components are not.
 */
export async function openBotLobby(page: Page, options: BotLobbyOptions = {}) {
  const players = options.players ?? 4;
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
  const seat = (index: number, occupant: string, nickname: string, extra: Partial<LobbySlot> = {}) => {
    slots[index] = { ...slots[index], occupant, nickname, ready: false, connected: true, ...extra };
  };
  seat(0, "host", HUMANS[0], { ready: true });
  if (options.guest || viewer === "guest_1") seat(1, "guest_1", HUMANS[1]);
  let botNumber = 0;
  const named = { random: 0, smart: 0 };
  const addBot = (kind: "random" | "smart") => {
    const index = slots.findIndex((slot) => !slot.occupant);
    if (index < 0) return false;
    botNumber += 1;
    named[kind] += 1;
    const name = `${kind === "smart" ? "Smart" : "Random"} bot ${named[kind]}`;
    seat(index, `bot_${botNumber}`, name, { ready: true, bot: kind });
    return true;
  };
  for (let i = 0; i < (options.initialBots ?? 0); i += 1) addBot(options.initialKind ?? "smart");

  const lobby = (): LobbyDto => ({
    game_id: GAME_ID,
    phase: "lobby",
    lobby_version: 3 + botNumber,
    host_player_id: "host",
    map: fixtureMapChoice(`${players}pStandard`, players),
    map_revision: 1,
    slots: slots.map((slot) => ({ ...slot })),
    bot_service_enabled: !!options.mlp,
    ...(options.botKinds ? { bot_kinds: options.botKinds } : {}),
  });

  await page.route(`**/api/games/${GAME_ID}/lobby/bots`, async (route) => {
    const body = route.request().postDataJSON() as {
      fill?: boolean;
      count?: number;
      kind?: "random" | "smart";
    };
    const wanted = body.fill ? slots.filter((slot) => !slot.occupant).length : (body.count ?? 1);
    for (let i = 0; i < wanted; i += 1) addBot(body.kind ?? "random");
    return route.fulfill({ json: lobby() });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/remove-bot`, async (route) => {
    const { player_id } = route.request().postDataJSON() as { player_id: string };
    const index = slots.findIndex((slot) => slot.occupant === player_id);
    if (index >= 0)
      slots[index] = { ...slots[index], occupant: null, nickname: null, ready: false, connected: false, bot: undefined };
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
      // The host has already seen the map picker for this table.
      sessionStorage.setItem(`ti4.map-picker-seen:${id}`, "1");
    },
    [GAME_ID, SESSION],
  );
  await page.goto(`/games/${GAME_ID}`);
}
