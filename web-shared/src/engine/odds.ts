// The odds of a combat, from the engine of the page (`{"query":"space_odds"}` and
// `{"query":"ground_odds"}` of `host_api::query`, on `ti4_calc::odds`): the same numbers online
// and offline, with no server. The engine is given as a query: `EngineLink.query` while the
// page plays a game in a Worker (the Worker asks the instance of the running game), else
// `pageQuery(module)`, an instance on the main thread.
import type {
  BattleResponse,
  GroundOddsRequest,
  GroundOddsResponse,
  SpaceOddsRequest,
} from "../wire/index.ts";
import { Host } from "./host.ts";

/** A query of the engine of the page (`EngineLink.query`, `Host.query`, `pageQuery`). */
export type EngineQuery = (request: object) => unknown;

/**
 * The odds of a space combat between two sides: 2000 rollouts from a fixed seed, so the same
 * sides give the same numbers. Rejects with the engine's error for sides it does not read.
 */
export async function spaceOdds(
  query: EngineQuery,
  request: SpaceOddsRequest,
): Promise<BattleResponse> {
  return (await query({ query: "space_odds", ...request })) as BattleResponse;
}

/**
 * The odds of a ground combat: the forces that land against the defender, with the guns of the
 * defender and the ships that bombard. Rejects with the engine's error for sides it does not
 * read.
 */
export async function groundOdds(
  query: EngineQuery,
  request: GroundOddsRequest,
): Promise<GroundOddsResponse> {
  return (await query({ query: "ground_odds", ...request })) as GroundOddsResponse;
}

/** The instance of a module that answers the queries of the main thread. It plays no game. */
const kept = new WeakMap<WebAssembly.Module, Promise<Host>>();

/**
 * A query of `module` on the main thread of the page, for a page that runs no game in a Worker
 * (an online game). One instance of the module answers every query; an instance that trapped is
 * replaced at the next one.
 */
export const pageQuery =
  (module: WebAssembly.Module): EngineQuery =>
  async (request) => {
    let host = kept.get(module);
    if (!host) {
      host = Host.create(module);
      kept.set(module, host);
      host.catch(() => kept.delete(module));
    }
    try {
      return (await host).query(request);
    } catch (error) {
      if (error instanceof WebAssembly.RuntimeError) {
        kept.delete(module);
      }
      throw error;
    }
  };
