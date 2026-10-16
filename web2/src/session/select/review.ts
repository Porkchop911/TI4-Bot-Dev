// The entry of a draft that the game refused, as the row that marks it in its step: what it
// names, by the board and the names of the units, and why it no longer fits.
import type { ReviewView, UnitType } from "../../model";
import { unitName } from "../../model";
import { type Review, removable } from "../draft";
import type { SessionUpdate, TacticalFacts } from "../wire";
import { systemLabel, tilesOf } from "./movement";
import { unitType } from "./names";
import { planetNames } from "./production";

const nameOf = (unit: string, damaged = false) =>
  unitName(unitType(unit) ?? "infantry", 1) + (damaged ? " (damaged)" : "");

/** Why a ship of the movement no longer fits, from the facts of the movement. */
function shipReason(
  update: SessionUpdate,
  facts: TacticalFacts | null,
  origin: string,
  unit: string,
  damaged: boolean,
): string | null {
  if (facts?.kind !== "movement") {
    return null;
  }
  const tiles = tilesOf(update);
  const ship = facts.ships.find(
    (item) => item.origin === origin && item.unit === unit && item.damaged === damaged,
  );
  return !ship
    ? `No longer in ${systemLabel(tiles, origin)}`
    : ship.blocked === "command_token"
      ? "Your command token is in its system"
      : !ship.move
        ? `Cannot reach ${systemLabel(tiles, facts.active)}`
        : null;
}

/** The row of the refused entry. `facts` are of the step that shows it. */
export function reviewRow(
  update: SessionUpdate,
  facts: TacticalFacts | null,
  review: Review,
  system: string | null,
): ReviewView {
  const tiles = tilesOf(update);
  const planets = planetNames(update);
  const planet = (id: string) => `Planet ${planets.get(id) ?? id}`;
  const entry = review.entry;
  const over = "The action ends before this";
  const generic =
    review.code === "over"
      ? over
      : review.code === "ambiguous"
        ? "The game offers this in more than one way"
        : review.code === "unpaid"
          ? "The payment does not cover the cost"
          : "The game no longer offers this";
  let icon: UnitType | null = null;
  let title: string;
  let reason = generic;
  switch (entry?.kind) {
    case "move":
      icon = unitType(entry.unit);
      title = `${nameOf(entry.unit, entry.damaged)} from ${systemLabel(tiles, entry.origin)}`;
      reason =
        review.code === "not_offered"
          ? (shipReason(update, facts, entry.origin, entry.unit, entry.damaged) ?? generic)
          : generic;
      break;
    case "load":
      icon = unitType(entry.unit);
      title = `${nameOf(entry.unit, entry.damaged)} loaded at ${systemLabel(tiles, entry.pickup_system)}${entry.source ? `, ${planet(entry.source)}` : ""}`;
      break;
    case "land":
      icon = unitType(entry.unit);
      title = `${nameOf(entry.unit, entry.damaged)} lands on ${planet(entry.planet)}`;
      break;
    case "produce":
      icon = unitType(entry.unit);
      title = `${nameOf(entry.unit)} produced`;
      break;
    case "place":
      title = `Placed in ${entry.destination === "space" ? "the space area" : planet(entry.destination)}`;
      break;
    case "pay":
      title =
        entry.source === "trade_good"
          ? "Paid with a trade good"
          : `Paid with ${planet(entry.source.replace(/^exhaust\|/, ""))}`;
      break;
    case "choose":
      title = "An answer before the step";
      break;
    case undefined:
      title = review.stage === null && system ? systemLabel(tiles, system) : "A recorded choice";
      reason = review.stage === null ? "Cannot be activated now" : generic;
      break;
    default:
      title = "The end of the step";
  }
  return {
    icon,
    title,
    reason,
    remove: removable(review) ? { label: "Remove", intent: { type: "removeEntry" } } : null,
  };
}
