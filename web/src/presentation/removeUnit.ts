import type { BoardView, ChoiceOptionDto, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { getUnitBaseType, getUnitDisplayName, type UnitBaseType } from "../components/UnitIcon.tsx";

/** What the seat is over: the fleet pool (ships) or the capacity of its ships (cargo). */
export type RemoveUnitReason = "fleet supply" | "capacity";

export interface RemoveUnitOption {
  /** "Remove Carrier": the unit's name in place of its raw id. */
  title: string;
  /** The unit type shown as an icon. */
  iconType: string;
  /** Where it is: "space of system 14 (2 Carriers there)". */
  where: string;
  /** What else goes with it, or null when nothing does. */
  effect: string | null;
}

export interface RemoveUnitView {
  reason: RemoveUnitReason;
  system: string;
  /** "3 over the fleet supply of 6 in system 14" */
  headline: string;
  /** Everything of the seat's in that space area, grouped: "2 Carriers, 4 Fighters". */
  present: string | null;
  option(option: Pick<ChoiceOptionDto, "id" | "label">): RemoveUnitOption;
}

const PROMPT = /^remove a unit: over (fleet supply|capacity) in (.+)$/;
/** Ships that provide capacity; removing one can strand the cargo above its remaining room. */
const CARRIES = new Set<UnitBaseType>(["carrier", "dreadnought", "flagship", "warsun"]);
const GROUND = new Set<UnitBaseType>(["infantry", "mech"]);

function num(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function group(types: string[]): string | null {
  const counts = new Map<UnitBaseType, number>();
  for (const type of types) {
    const base = getUnitBaseType(type);
    counts.set(base, (counts.get(base) ?? 0) + 1);
  }
  if (counts.size === 0) return null;
  return [...counts.entries()].map(([base, n]) => `${n} ${getUnitDisplayName(base, n)}`).join(", ");
}

/** True when the decision is the engine's "remove a unit" question. */
export function isRemoveUnitChoice(choice: Pick<PendingChoiceDto, "prompt">): boolean {
  return PROMPT.test(choice.prompt);
}

/**
 * Names, places and consequences for "remove a unit: over fleet supply in 14". Display only: the
 * options and their ids are untouched. `null` for any other decision.
 */
export function describeRemoveUnit(
  choice: Pick<PendingChoiceDto, "prompt" | "details" | "actor">,
  board: BoardView | null | undefined,
  player?: Pick<PlayerView, "fleet_tokens"> | null,
): RemoveUnitView | null {
  const match = PROMPT.exec(choice.prompt);
  if (!match) return null;
  const reason = match[1] as RemoveUnitReason;
  const system = match[2];
  const label = `system ${system}`;

  const space = (board?.systems?.[system]?.units ?? []).filter(
    (u) => u.owner === choice.actor && !u.planet,
  );
  const spaceTypes = space.map((u) => u.unit_type);
  const bases = spaceTypes.map(getUnitBaseType);
  const cargo = space
    .filter((u) => {
      const b = getUnitBaseType(u.unit_type);
      return b === "fighter" || GROUND.has(b);
    })
    .map((u) => u.unit_type);
  const cargoText = group(cargo);
  const shipCount = bases.filter(
    (b) => b !== "fighter" && !GROUND.has(b) && b !== "pds" && b !== "spacedock",
  ).length;

  let headline: string;
  if (reason === "fleet supply") {
    const limit = num(choice.details?.fleet_limit) ?? player?.fleet_tokens ?? null;
    const charged = num(choice.details?.fleet_charged) ?? (board ? shipCount : null);
    headline =
      limit !== null && charged !== null
        ? `${Math.max(1, charged - limit)} over the fleet supply of ${limit} (${charged} ships counted) in ${label}`
        : `Over the fleet supply in ${label}`;
  } else {
    const consumed = num(choice.details?.capacity_consumed);
    const transport = num(choice.details?.capacity_transport);
    headline =
      consumed !== null && transport !== null
        ? `${Math.max(1, consumed - transport)} over capacity in ${label}: ${consumed} fighters and ground forces for ${transport} places`
        : `Over capacity in ${label}`;
  }

  return {
    reason,
    system,
    headline,
    present: group(spaceTypes),
    option(option) {
      const type = option.label.replace(/^remove\s+/i, "").trim() || option.label;
      const base = getUnitBaseType(type);
      const name = getUnitDisplayName(type);
      const same = bases.filter((b) => b === base).length;
      const where =
        same > 0
          ? `In the space of ${label}${same > 1 ? ` (${same} ${getUnitDisplayName(base, same)} there)` : ""}`
          : `In the space of ${label}`;
      let effect: string | null = null;
      if (CARRIES.has(base) && cargoText) {
        effect = `Carries up to its capacity: ${cargoText} are in this space. Any that no longer fit are removed next.`;
      } else if (CARRIES.has(base)) {
        effect = "Nothing in this space needs its capacity.";
      } else if (base === "fighter" || GROUND.has(base)) {
        effect = "Frees one place of capacity.";
      } else if (reason === "fleet supply") {
        effect = "Frees one place in the fleet supply.";
      }
      return { title: `Remove ${name}${/2$/.test(type) ? " II" : ""}`, iconType: type, where, effect };
    },
  };
}
