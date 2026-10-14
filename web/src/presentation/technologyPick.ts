import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { TECH_TRACKS, getTechnologyTrack, hydrateTech, type PrereqSymbol } from "./technologyData.ts";

const TECH_PICK = /_pick_technology$/;

/** A card asking which technology: Divert Funding returns one, then researches another. */
export const isTechnologyPick = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  TECH_PICK.test(choice.context?.subtype ?? "");

export interface TechnologyPickOption {
  name: string;
  /** "Biotic", or null when the technology has no colour. */
  track: string | null;
  accentColor: string | null;
  /** The prerequisites it needs (what researching it takes), as coloured symbols. */
  prereqs: PrereqSymbol[];
  /** The printed text, one paragraph per entry. */
  text: string[];
}

/** One technology option as the card prints it. `null` for options that are not technologies. */
export function describeTechnologyPickOption(
  choice: Pick<PendingChoiceDto, "context">,
  option: Pick<ChoiceOptionDto, "id" | "kind">,
): TechnologyPickOption | null {
  if (!isTechnologyPick(choice) || option.kind !== "technology") return null;
  const tech = hydrateTech(option.id);
  const trackId = getTechnologyTrack(option.id);
  const track = TECH_TRACKS.find((entry) => entry.id === trackId) ?? null;
  return {
    name: tech.meta.name,
    track: track?.name ?? null,
    accentColor: track?.accentColor ?? null,
    prereqs: tech.prereqs,
    text: (tech.meta.description ?? "")
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0),
  };
}

/** What this question of the card does, by its prompt (the same subtype serves both halves). */
export function technologyPickStage(
  choice: Pick<PendingChoiceDto, "context" | "prompt">,
): { title: string; text: string } | null {
  if (!isTechnologyPick(choice)) return null;
  if (/which technology to return/i.test(choice.prompt)) {
    return {
      title: "Return a technology",
      text: "The technology leaves your play area and goes back to the technology deck. Only a non-unit-upgrade, non-faction technology can be returned. You then research another one.",
    };
  }
  if (/what to research/i.test(choice.prompt)) {
    return {
      title: "Research another technology",
      text: "Prerequisites still apply. These are the technologies you can research now.",
    };
  }
  return null;
}
