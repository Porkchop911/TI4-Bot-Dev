import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findPlanetMeta, getTechnologyMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";

/** One labelled fact of an offer card. */
export interface OfferFact {
  label: string;
  /** Plain text (a name, an amount). */
  text: string | null;
  /** A unit type: shown with its icon. */
  unit: string | null;
  /** A seat: shown by the player's name (resolved with the player identity). */
  seat: string | null;
  /** A before/after number, e.g. commodities 1 → 2 of 3. */
  change: { from: number; to: number; of: number | null } | null;
}

export interface OfferAnswer {
  option: ChoiceOptionDto;
  /** What the button says (the engine's caption, else the option's own label). */
  label: string;
  /** What it does, under the label. */
  hint: string | null;
  /** The seat the option is about (a unit of theirs), shown beside the hint. */
  seat: string | null;
  /** The stop/decline answer, shown as the quiet button. */
  isDecline: boolean;
}

export interface OfferCardView {
  title: string;
  /** "commander", "faction ability", ... */
  tag: string;
  window: string | null;
  text: string | null;
  facts: OfferFact[];
  answers: OfferAnswer[];
}

const text = (value: unknown): string | null =>
  typeof value === "string" && value.trim() ? value.trim() : null;

const num = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

const capitalize = (value: string): string => (value ? value[0].toUpperCase() + value.slice(1) : value);

const isDecline = (option: ChoiceOptionDto) => option.kind === "decline" || option.id === "decline";

function readFact(raw: unknown): OfferFact | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const label = text(record.label);
  if (!label) return null;
  const unit = text(record.unit);
  const planetId = text(record.planet);
  const technologyId = text(record.technology);
  const seat = text(record.seat);
  const system = text(record.system);
  const from = num(record.from);
  const to = num(record.to);
  let value: string | null = text(record.value);
  if (value === null && typeof record.value === "number") value = String(record.value);
  if (planetId) {
    const name = findPlanetMeta(planetId)?.name ?? humanizeId(planetId);
    value = system ? `${name} (system ${system})` : name;
  }
  if (technologyId) value = getTechnologyMeta(technologyId).name;
  if (unit) value = null;
  const change = from !== null && to !== null ? { from, to, of: num(record.of) } : null;
  if (!unit && !seat && !change && value === null) return null;
  return { label, text: unit || seat || change ? null : value, unit, seat, change };
}

/** "Sustain damage" for a unit: the display name, the way unit lists show it. */
export const offerUnitName = (type: string): string =>
  `${getUnitDisplayName(type)}${/2$/.test(type) ? " II" : ""}`;

/**
 * A decision the engine presented as an offer card (`details.kind === "offer"`): what is asked, a
 * few labelled facts and a caption for each answer. `null` for any other decision, or without the
 * card header.
 */
export function describeOfferCard(
  choice: Pick<PendingChoiceDto, "options" | "details">,
): OfferCardView | null {
  const details = choice.details;
  if (!details || details.kind !== "offer") return null;
  const card = details.card as Record<string, unknown> | undefined;
  const title = text(card?.title);
  if (!title || choice.options.length === 0) return null;
  const captions = (details.captions ?? {}) as Record<string, Record<string, unknown>>;
  const facts = Array.isArray(details.facts)
    ? details.facts.map(readFact).filter((fact): fact is OfferFact => fact !== null)
    : [];
  const answers: OfferAnswer[] = choice.options.map((option) => {
    const caption = captions[option.id];
    return {
      option,
      label: text(caption?.label) ?? capitalize(option.label ?? option.id),
      hint: text(caption?.hint),
      seat: text(caption?.seat),
      isDecline: isDecline(option),
    };
  });
  // The decision answers last, so the quiet button sits at the end of the row.
  answers.sort((a, b) => Number(a.isDecline) - Number(b.isDecline));
  return {
    title,
    tag: text(card?.tag) ?? "decision",
    window: text(card?.window),
    text: text(card?.text),
    facts,
    answers,
  };
}
