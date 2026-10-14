import type {
  DecisionContextDto,
  HitCauseDto,
  HitDetailDto,
} from "../protocol/types.ts";
import { getUnitBaseType } from "../components/UnitIcon.tsx";

/**
 * What a casualty decision means, in words. Derived from the engine's structured `context.hit`
 * (cause, destroy-vs-hit, restriction); the subtype is only the fallback for servers that do not
 * send it yet. Never from label text.
 */
export interface HitKindModel {
  cause: HitCauseDto;
  icon: string;
  /** Source name shown as the badge: "Assault Cannon". */
  source: string;
  /** "destroy" is not a hit: Sustain Damage cannot cancel it. */
  tone: "destroy" | "hit";
  headline: string;
  /** One chip each: kind, restriction, sustain. */
  chips: { text: string; tone: "danger" | "info" | "neutral" }[];
  restriction: "any" | "non_fighter";
}

export interface HitKindNames {
  /** The seat that produced the hits, when known. */
  producer?: string;
  /** Hits still owed, when the engine states it. */
  hits?: number;
}

const SOURCES: Record<HitCauseDto, { icon: string; source: string }> = {
  assault_cannon: { icon: "🔻", source: "Assault Cannon" },
  courageous_to_the_end: { icon: "🎯", source: "Courageous to the End" },
  anti_fighter_barrage: { icon: "🛰️", source: "Anti-Fighter Barrage" },
  space_cannon: { icon: "☄️", source: "Space Cannon" },
  combat_roll: { icon: "💥", source: "Combat hits" },
  start_of_combat: { icon: "⚡", source: "Start-of-combat hits" },
  end_of_round: { icon: "⚡", source: "End-of-round hits" },
  other: { icon: "💥", source: "Hits" },
};

/** The structured hit description, falling back to the subtype for older payloads. */
export function hitDetailOf(
  context: DecisionContextDto | null | undefined,
): HitDetailDto | null {
  if (!context) return null;
  if (context.hit) return context.hit;
  switch (context.subtype) {
    case "assault_cannon_destroy":
      return {
        cause: "assault_cannon",
        destroy: true,
        restriction: "non_fighter",
      };
    case "courageous_to_the_end_assign_casualty":
      return {
        cause: "courageous_to_the_end",
        destroy: true,
        restriction: "any",
      };
    default:
      return null;
  }
}

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);

export function deriveHitKind(
  context: DecisionContextDto | null | undefined,
  names: HitKindNames,
): HitKindModel | null {
  const hit = hitDetailOf(context);
  if (!hit) return null;
  const { icon, source } = SOURCES[hit.cause] ?? SOURCES.other;
  const producer = names.producer ?? "your opponent";
  const n = names.hits ?? 1;
  const restriction = hit.restriction;

  let headline: string;
  if (hit.cause === "assault_cannon") {
    headline = `${source}: ${producer} destroys 1 of your non-fighter ships`;
  } else if (hit.cause === "courageous_to_the_end") {
    headline = `${source}: destroy 1 of your ships (${producer}'s die hit)`;
  } else if (hit.destroy) {
    headline = `${source}: destroy ${n} of your ships`;
  } else {
    headline = `${source}: ${n} ${plural(n, "hit", "hits")} from ${producer} to assign`;
  }

  const chips: HitKindModel["chips"] = [
    hit.destroy
      ? { text: "Destroy, not a hit", tone: "danger" }
      : { text: "Normal hit", tone: "info" },
    restriction === "non_fighter"
      ? { text: "Non-fighter ships only", tone: "neutral" }
      : {
          text:
            hit.cause === "anti_fighter_barrage"
              ? "Any ship (Waylay)"
              : "Any ship",
          tone: "neutral",
        },
    hit.destroy
      ? { text: "Cannot be sustained", tone: "neutral" }
      : context?.subtype === "sustain_damage"
        ? { text: "Sustain Damage can cancel it", tone: "neutral" }
        : { text: "Sustain was offered first", tone: "neutral" },
  ];

  return {
    cause: hit.cause,
    icon,
    source,
    tone: hit.destroy ? "destroy" : "hit",
    headline,
    chips,
    restriction,
  };
}

/** Why a ship cannot take this hit, for a tooltip; null when it can (or nothing is restricted). */
export function illegalTargetReason(
  model: Pick<HitKindModel, "restriction"> | null,
  unitType: string,
): string | null {
  if (!model) return null;
  if (
    model.restriction === "non_fighter" &&
    getUnitBaseType(unitType) === "fighter"
  ) {
    return "Cannot be assigned: not a valid target, this hit needs a non-fighter ship";
  }
  return null;
}

/** Anti-fighter barrage window (the Waylay offer before a side's barrage dice). */
export interface BarrageWindowModel {
  icon: string;
  source: string;
  headline: string;
  lines: string[];
}

export function isBarrageWindow(
  context: DecisionContextDto | null | undefined,
): boolean {
  return (
    context?.trigger?.kind === "anti_fighter_barrage" ||
    context?.trigger?.event_type === "ANTI_FIGHTER_BARRAGE_STARTED" ||
    context?.subtype === "reaction_when_ANTI_FIGHTER_BARRAGE_STARTED"
  );
}

export function deriveBarrageWindow(
  context: DecisionContextDto | null | undefined,
  roller: string | null,
): BarrageWindowModel | null {
  if (!isBarrageWindow(context)) return null;
  const who = roller ?? "a player";
  return {
    icon: SOURCES.anti_fighter_barrage.icon,
    source: SOURCES.anti_fighter_barrage.source,
    headline: `Anti-Fighter Barrage: before ${who} rolls`,
    lines: [
      "Each destroyer rolls its barrage dice, and every hit destroys one of the opposing fighters. It is automatic: nobody is asked which fighter, and it cannot be sustained. Hits beyond the fighters are lost.",
      "Waylay is the exception: it makes this roll's hits count against all ships.",
    ],
  };
}
