// The fleet supply and the capacity of a seat in a system, from the engine of the page
// (`{"query":"fleet_standing"}` of `host_api::query`: `ti4_engine::fleet::FleetCounts::standing`,
// whose `Standing` is `FleetStanding` on the wire): a client that stages units before it sends them adds them to the counts of the engine, and the
// engine reads them by its rules (the fighters that a space dock holds, Fighter II).
import type { FleetCounts, FleetStanding } from "../wire/index.ts";
import type { EngineQuery } from "./odds.ts";

/** The standing of some counts. Nothing of a game is read. */
export async function fleetStanding(
  query: EngineQuery,
  counts: FleetCounts,
): Promise<FleetStanding> {
  return (await query({ query: "fleet_standing", ...counts })) as FleetStanding;
}
