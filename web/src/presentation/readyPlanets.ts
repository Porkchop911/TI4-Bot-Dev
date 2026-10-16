import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import type { BasketPlan } from "../protocol/client.ts";
import { isReadyPlanetChoice } from "./choiceModel.ts";
import { getPlanetDetails } from "./planetSelection.ts";
import { planetLabel } from "./secondaryPlan.ts";

/** Diplomacy readies up to this many exhausted planets (primary and secondary alike). */
export const READY_PLANET_LIMIT = 2;

export interface ReadyPlanet {
  id: string;
  name: string;
  resources: number | null;
  influence: number | null;
}

/**
 * Diplomacy's "ready which planet" question as one choice: the engine asks it once per planet
 * (up to two, and never declinable), the UI shows the exhausted planets at once and sends the
 * chosen ones together as a `ready_planets` batch.
 */
export interface ReadyPlanetsView {
  planets: ReadyPlanet[];
  /**
   * How many planets must be picked: the engine keeps asking until two are readied or nothing
   * is left to ready, so this is two, or fewer when fewer planets are exhausted.
   */
  required: number;
}

/** The planets view for a `ready_planet` question; null for any other decision. */
export function describeReadyPlanets(
  choice: PendingChoiceDto | null | undefined,
  board?: BoardView | null,
): ReadyPlanetsView | null {
  if (!choice || !isReadyPlanetChoice(choice)) return null;
  const planets = choice.options.map((option): ReadyPlanet => {
    const details = getPlanetDetails(option.id, board);
    const known = details.name && details.name !== option.id ? details.name : planetLabel(option.id);
    return { id: option.id, name: known, resources: details.resources, influence: details.influence };
  });
  return { planets, required: Math.min(READY_PLANET_LIMIT, planets.length) };
}

/**
 * Flips a planet in the selection. A full selection drops its oldest pick for a new one, so a
 * tap on a third planet swaps instead of doing nothing; a planet the question does not offer
 * changes nothing.
 */
export function toggleReadyPlanet(
  view: ReadyPlanetsView,
  selected: readonly string[],
  planetId: string,
): string[] {
  if (!view.planets.some((planet) => planet.id === planetId)) return [...selected];
  if (selected.includes(planetId)) return selected.filter((id) => id !== planetId);
  const next = [...selected, planetId];
  return next.length > view.required ? next.slice(next.length - view.required) : next;
}

/** The picks that the question still offers, in the order picked. */
export function validReadySelection(view: ReadyPlanetsView, selected: readonly string[]): string[] {
  const offered = new Set(view.planets.map((planet) => planet.id));
  return selected.filter((id, at) => offered.has(id) && selected.indexOf(id) === at).slice(0, view.required);
}

export const canConfirmReady = (view: ReadyPlanetsView, selected: readonly string[]): boolean =>
  view.required > 0 && validReadySelection(view, selected).length === view.required;

/** The batch for a complete selection, or null while planets are still owed. */
export function readyPlan(view: ReadyPlanetsView, selected: readonly string[]): BasketPlan | null {
  if (!canConfirmReady(view, selected)) return null;
  return {
    kind: "ready_planets",
    steps: validReadySelection(view, selected).map((planet) => ({ kind: "ready_planet", planet })),
  };
}
