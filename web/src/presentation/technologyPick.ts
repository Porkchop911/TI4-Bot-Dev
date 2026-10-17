import type { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { TECH_TRACKS, getTechnologyTrack, hydrateTech, type PrereqSymbol } from "./technologyData.ts";

const TECH_PICK = /_pick_technology$/;
/** Setup: the technology a faction starts the game with (the engine's `starting_technology`). */
export const STARTING_TECHNOLOGY = "starting_technology";

/** A card asking which technology: Divert Funding returns one, then researches another. */
export const isTechnologyPick = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  TECH_PICK.test(choice.context?.subtype ?? "") || choice.context?.subtype === STARTING_TECHNOLOGY;

export interface TechnologyPickOption {
  name: string;
  /** "Biotic", or null when the technology has no colour. */
  track: string | null;
  accentColor: string | null;
  /** The prerequisites it needs (what researching it takes), as coloured symbols. */
  prereqs: PrereqSymbol[];
  /** The printed text, one paragraph per entry. */
  text: string[];
  /** The seat the technology is taken from (Plagiarize), or null when the pick has no owner. */
  owner: string | null;
  /** The seats that hold the technology, for a starting pick that copies the others' (Keleres). */
  holders: string[];
}

/** Plagiarize offers `<seat>|<technology>`: the owner and the technology the option stands for. */
export function splitTechnologyOptionId(id: string): { owner: string | null; technology: string } {
  const at = id.indexOf("|");
  return at < 0 ? { owner: null, technology: id } : { owner: id.slice(0, at), technology: id.slice(at + 1) };
}

/** One technology option as the card prints it. `null` for options that are not technologies. */
export function describeTechnologyPickOption(
  choice: Pick<PendingChoiceDto, "context">,
  option: Pick<ChoiceOption, "id" | "kind"> & Partial<Pick<ChoiceOption, "payload">>,
): TechnologyPickOption | null {
  if (!isTechnologyPick(choice)) return null;
  if (option.kind !== "technology" && option.kind !== STARTING_TECHNOLOGY) return null;
  const { owner, technology } = splitTechnologyOptionId(option.id);
  const tech = hydrateTech(technology);
  const trackId = getTechnologyTrack(technology);
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
    owner,
    holders: Array.isArray(option.payload?.owners)
      ? option.payload.owners.filter((seat): seat is string => typeof seat === "string")
      : [],
  };
}

/** What a starting-technology question says, from the engine's `details` (pick, of, rule). */
export function startingTechnologyStage(
  choice: Pick<PendingChoiceDto, "details" | "options">,
): { title: string; text: string } {
  const details = (choice.details ?? {}) as Record<string, unknown>;
  const number = typeof details.pick === "number" ? details.pick : 1;
  const of = typeof details.of === "number" ? details.of : 1;
  const rule = typeof details.rule === "string" ? details.rule : "listed";
  const held = Array.isArray(details.held) ? details.held.filter((t): t is string => typeof t === "string") : [];
  const title = of > 1 ? `Starting technology · ${number} of ${of}` : "Starting technology";
  const kept = held.length
    ? ` You already start with ${held.map((id) => hydrateTech(id).meta.name).join(", ")}.`
    : "";
  if (rule === "owned_by_others") {
    return {
      title,
      text: `Choose a non-faction technology that another player starts with; you gain it and they keep theirs. The Council Keleres choose after every other seat.${kept}`,
    };
  }
  if (rule === "researchable") {
    return {
      title,
      text: `Research a technology now, without paying: its prerequisites still apply, and the first one you take can open the second.${kept}`,
    };
  }
  const left = choice.options.length - (of - number);
  return {
    title,
    text:
      of > 1
        ? `Choose ${of} of these technologies. The one you leave behind stays in the technology deck.${kept}`
        : left > 1
          ? `Choose one technology. You start the game with it.${kept}`
          : `You start the game with this technology.${kept}`,
  };
}

/** What this question of the card does, by its prompt (the same subtype serves both halves). */
export function technologyPickStage(
  choice: Pick<PendingChoiceDto, "context" | "prompt"> &
    Partial<Pick<PendingChoiceDto, "details" | "options">>,
): { title: string; text: string } | null {
  if (!isTechnologyPick(choice)) return null;
  if (choice.context?.subtype === STARTING_TECHNOLOGY) {
    return startingTechnologyStage({ details: choice.details, options: choice.options ?? [] });
  }
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
  if (/which technology to steal/i.test(choice.prompt)) {
    return {
      title: "Steal a technology",
      text: "You have paid 5 influence. Choose a non-faction technology one of your neighbours owns; you gain it and they keep theirs.",
    };
  }
  return null;
}
