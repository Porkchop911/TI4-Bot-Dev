import { TECH_TRACKS, getTechnologyTrack, type HydratedTech } from "./technologyData.ts";

/** A colour chip of the technology filter: one of the four tracks, or unit upgrades. */
export type TechColorFilter = (typeof TECH_TRACKS)[number]["id"] | "UNIT_UPGRADE";

export interface TechFilter {
  /** Show only this colour; null shows all of them. */
  color: TechColorFilter | null;
  /** Show only technologies the viewer can research right now. */
  availableOnly: boolean;
}

export const NO_TECH_FILTER: TechFilter = { color: null, availableOnly: false };

export const isTechFilterActive = (filter: TechFilter): boolean =>
  filter.color !== null || filter.availableOnly;

/** The colour a technology belongs to (faction technologies carry theirs in their types). */
export function techColor(tech: HydratedTech): TechColorFilter | null {
  const track = getTechnologyTrack(tech.id);
  if (track) return track;
  const types = (tech.meta.types ?? []).map((type) => type.toLowerCase().replace(/_/g, ""));
  if (types.includes("unitupgrade")) return "UNIT_UPGRADE";
  for (const candidate of TECH_TRACKS) {
    if (types.includes(candidate.id.toLowerCase())) return candidate.id;
  }
  return null;
}

/** Whether a card stays visible under the filter; `available` says if the viewer can research it now. */
export function matchesTechFilter(tech: HydratedTech, filter: TechFilter, available: boolean): boolean {
  if (filter.color !== null && techColor(tech) !== filter.color) return false;
  if (filter.availableOnly && !available) return false;
  return true;
}
