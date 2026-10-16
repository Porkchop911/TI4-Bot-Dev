import type { Page, Route } from "@playwright/test";
import type {
  LobbyDto,
  MapChoice,
  MapChoiceView,
  MapPreview,
  MapTemplateSummary,
  StrategyCardSetId,
} from "../../../src/protocol/types";
import { fixtureLobby, fixturePreview, fixtureTemplates } from "../../../src/dev/mapPickerFixtures";
import { GAME_ID, SESSION } from "./mockGame";

export interface MapLobbyOptions {
  players?: number;
  /** Alias of the chosen template; null = Random. */
  alias?: string | null;
  /** The viewer: the host ("host", seat 1) or another player ("guest_1", seat 2). */
  viewer?: "host" | "guest_1";
  /** Map revision (0 = the host has not chosen yet). */
  revision?: number;
  catalog?: "ready" | "loading" | "error";
  preview?: "ready" | "loading" | "error";
  /** Reject a Random choice, as the server does when no layout fits. */
  rejectRandom?: boolean;
  /** The strategy card set the lobby reports (default: omitted, as an older server would). */
  strategyCardSet?: StrategyCardSetId;
  /**
   * Real server answers instead of the synthetic fixtures: the catalog for this player count and
   * the preview of each alias (missing aliases fall back to the fixture board). The lobby's map
   * description is then taken from the catalog entry, as the server's `describe` does.
   */
  real?: { templates: MapTemplateSummary[]; previews: Record<string, MapPreview> };
}

/** Moves the filler systems around the ring, so a re-roll draws a visibly different board. */
function shuffled(preview: ReturnType<typeof fixturePreview>, variant: number) {
  const free = preview.tiles.filter((t) => !t.system_id.startsWith("home-") && t.system_id !== "18");
  const props = free.map(({ system_id, label, planets, anomalies, wormholes }) => ({ system_id, label, planets, anomalies, wormholes }));
  const shift = (variant * 7) % free.length;
  free.forEach((tile, i) => {
    const p = props[(i + shift) % props.length];
    Object.assign(tile, { anomalies: undefined, wormholes: undefined }, p);
  });
  return preview;
}

/**
 * Renders the real lobby at /games/<id> with the real MapPicker behind it, against routed HTTP
 * (maps catalog, lobby, map choice, map preview): the server is mocked, the components are not.
 * Every POST /lobby/map bumps the map revision and re-rolls the pictured board.
 */
export async function openMapLobby(page: Page, options: MapLobbyOptions = {}) {
  const players = options.players ?? 6;
  const viewer = options.viewer ?? "host";
  let alias: string | null = options.alias === undefined ? `${players}pStandard` : options.alias;
  let revision = options.revision ?? 1;
  const realChoice = (): MapChoiceView | undefined => {
    const entry = options.real?.templates.find((t) => t.alias === alias);
    if (!entry) return undefined;
    const { name, author, systems, hyperlanes, recommended } = entry;
    return { kind: "template", alias: entry.alias, ...(name ? { name } : {}), author, systems, hyperlanes, recommended };
  };
  const previewOf = (name: string | null) =>
    (name && options.real?.previews[name]) || fixturePreview(players, name);
  const lobby = (): LobbyDto => ({
    ...fixtureLobby(players, alias, revision),
    ...(realChoice() ? { map: realChoice() } : {}),
    game_id: GAME_ID,
    ...(options.strategyCardSet ? { strategy_card_set: options.strategyCardSet } : {}),
  });
  const hang = () => new Promise<void>(() => undefined);
  const fail = (route: Route) => route.fulfill({ status: 503, json: { error: "server unavailable" } });

  await page.route("**/api/maps?*", async (route) => {
    if (options.catalog === "loading") return hang();
    if (options.catalog === "error") return fail(route);
    // Seven seats: no ready-made layout fits.
    if (options.real) return route.fulfill({ json: options.real.templates });
    return route.fulfill({ json: players > 6 ? [] : fixtureTemplates(players) });
  });
  // The small picture on each choice card.
  await page.route("**/api/maps/*/preview?*", (route) => {
    const name = /\/api\/maps\/([^/?]+)\/preview/.exec(route.request().url())?.[1] ?? "random";
    return route.fulfill({ json: previewOf(name === "random" ? null : decodeURIComponent(name)) });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/map-preview`, async (route) => {
    if (options.preview === "loading") return hang();
    if (options.preview === "error") return fail(route);
    if (alias && options.real?.previews[alias]) return route.fulfill({ json: options.real.previews[alias] });
    return route.fulfill({ json: shuffled(fixturePreview(players, alias), revision) });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/map`, async (route) => {
    const { map } = route.request().postDataJSON() as { map: MapChoice };
    if (map.kind === "random" && options.rejectRandom)
      return route.fulfill({ status: 422, body: "no map fits 7 players" });
    alias = map.kind === "random" ? null : map.alias;
    revision += 1;
    return route.fulfill({ json: lobby() });
  });
  await page.route(`**/api/games/${GAME_ID}/lobby/join`, (route) =>
    route.fulfill({ json: { player_session: SESSION, player: { id: viewer }, lobby: lobby() } }),
  );
  await page.route(`**/api/games/${GAME_ID}/lobby/heartbeat`, (route) => route.fulfill({ json: {} }));
  await page.route(`**/api/games/${GAME_ID}/lobby`, (route) => route.fulfill({ json: lobby() }));
  await page.goto("/");
  await page.evaluate(([id, credential]) => sessionStorage.setItem(`ti4.player-session:${id}`, credential), [GAME_ID, SESSION]);
  await page.goto(`/games/${GAME_ID}`);
}
