import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findActionCardMeta, findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";
import { systemNamedLabel } from "./systemFacts.ts";
import type { BoardView } from "../protocol/types.ts";

const UNIT_PICK = /_pick_(infantry|ship|cruiser)$/;

/** Refit Troops, Scuttle, Upgrade and Lucky Shot: pick a unit on the board (yours, or for Lucky Shot a rival's). */
export const isUnitPick = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  UNIT_PICK.test(choice.context?.subtype ?? "");

export interface UnitPickOption {
  /** "Dreadnought II", in place of the raw unit id. */
  title: string;
  /** The unit type, shown as an icon. */
  iconType: string;
  /** "In the space of system 14" or "On Jord (system 14)". */
  where: string;
  /** What the card does to this unit. */
  effect: string;
  /** The seat that owns the unit when it is not the one choosing (Lucky Shot's targets), else null. */
  owner: string | null;
  /** The system the unit stands in, so a client can mark it on the map. */
  systemId: string;
}

const text = (value: unknown): string | null =>
  typeof value === "string" && value ? value : null;

/** One unit option with its place and what the card does to it; `null` for the stop option or without details. */
export function describeUnitPickOption(
  choice: Pick<PendingChoiceDto, "context" | "details"> & Partial<Pick<PendingChoiceDto, "actor">>,
  option: Pick<ChoiceOptionDto, "id">,
  board?: BoardView | null,
): UnitPickOption | null {
  if (!isUnitPick(choice)) return null;
  const units = choice.details?.units as Record<string, Record<string, unknown>> | undefined;
  const info = units?.[option.id];
  const type = text(info?.unit);
  const system = text(info?.system);
  if (!info || !type || !system) return null;
  const planetId = text(info.planet);
  const planet = planetId ? (findPlanetMeta(planetId)?.name ?? humanizeId(planetId)) : null;
  const damaged = info.damaged === true ? " (damaged)" : "";
  const name = `${getUnitDisplayName(type)}${/2$/.test(type) ? " II" : ""}`;
  const card = (choice.context?.subtype ?? "").split("_pick_")[0];
  const cost = typeof info.cost === "number" && Number.isFinite(info.cost) ? Math.round(info.cost) : null;
  const owner = text(info.owner);
  const place = board ? systemNamedLabel(system, board) : `system ${system}`;
  let effect: string;
  if (card === "lucky") {
    effect = "Destroyed: removed from the board and returned to its owner's reinforcements.";
  } else if (card === "upgrade") {
    effect = "Replaced by a dreadnought from your reinforcements.";
  } else if (choice.context?.subtype?.endsWith("_pick_infantry") === true) {
    effect = "Replaced by a mech from your reinforcements.";
  } else if (cost !== null) {
    effect = `Returned to your reinforcements; you gain ${cost} trade good${cost === 1 ? "" : "s"}.`;
  } else {
    effect = "Returned to your reinforcements; you gain trade goods equal to its cost.";
  }
  return {
    title: `${name}${damaged}`,
    iconType: type,
    where: planet ? `On ${planet} (${place})` : `In the space of ${place}`,
    effect,
    owner: owner && owner !== choice.actor ? owner : null,
    systemId: system,
  };
}

/** The action card behind a unit pick, as printed; `null` for any other decision. */
export function describeUnitPickCard(
  choice: Pick<PendingChoiceDto, "context">,
): { name: string; text: string } | null {
  if (!isUnitPick(choice)) return null;
  const id = (choice.context?.source as Record<string, unknown> | undefined)?.ActionCard;
  if (typeof id !== "string") return null;
  const meta = findActionCardMeta(id);
  return { name: meta?.name ?? humanizeId(id), text: meta?.description ?? "" };
}
