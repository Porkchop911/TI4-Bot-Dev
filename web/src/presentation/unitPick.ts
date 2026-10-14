import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findActionCardMeta, findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";

const UNIT_PICK = /_pick_(infantry|ship)$/;

/** Refit Troops and Scuttle: pick one of your own units on the board. */
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
}

const text = (value: unknown): string | null =>
  typeof value === "string" && value ? value : null;

/** One unit option with its place and what the card does to it; `null` for the stop option or without details. */
export function describeUnitPickOption(
  choice: Pick<PendingChoiceDto, "context" | "details">,
  option: Pick<ChoiceOptionDto, "id">,
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
  const refit = choice.context?.subtype?.endsWith("_pick_infantry") === true;
  const cost = typeof info.cost === "number" && Number.isFinite(info.cost) ? Math.round(info.cost) : null;
  return {
    title: `${name}${damaged}`,
    iconType: type,
    where: planet ? `On ${planet} (system ${system})` : `In the space of system ${system}`,
    effect: refit
      ? "Replaced by a mech from your reinforcements."
      : cost !== null
        ? `Returned to your reinforcements; you gain ${cost} trade good${cost === 1 ? "" : "s"}.`
        : "Returned to your reinforcements; you gain trade goods equal to its cost.",
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
