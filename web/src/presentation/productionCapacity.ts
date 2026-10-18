import type { ChoiceOption } from "../protocol/types.ts";

/**
 * Capacity (LRR 16) of the system a production dialog builds in, with the staged builds added.
 *
 * Every number is the engine's: the produce question carries the system's present capacity
 * (`details.capacity`: transport, consumed, unused dock support) and each build option names what
 * it gives and takes (`capacity_provided`, `capacity_needed`, `capacity_role`, `space_placement`),
 * so unit statistics, Carrier II, faction and technology modifiers are never re-derived here.
 * This module only adds them up. Display only: nothing here blocks a build.
 */

export interface CapacityNow {
  transport: number;
  consumed: number;
  fighters: number;
  unusedSupport: number;
}

export type CapacityRole = "fighter" | "ground" | "none";
export type SpacePlacement = "always" | "never" | "choice";

export interface CapacityFacts {
  provided: number;
  needed: number;
  role: CapacityRole;
  placement: SpacePlacement;
}

export interface CapacityMeter {
  /** Capacity of the ships already in the system. */
  existing: number;
  /** Capacity the staged ships add. */
  fromQueued: number;
  /** Capacity in total (existing + queued ships). */
  transport: number;
  /** Units in the space area that need it, after dock fighter support, staged ones included. */
  consumed: number;
  /** Fighters and ground forces already needing capacity, before this build. */
  consumedNow: number;
  queuedFighters: number;
  queuedGround: number;
  /** Staged ground forces whose destination is the owner's choice (planet or space). */
  maybeGround: number;
  /** Units that do not fit; they would be removed at the end of production. */
  over: number;
  /** What the units over capacity are called: only fighters, or fighters and ground forces. */
  overKind: "fighters" | "units";
}

const count = (value: unknown): number | null =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;

export function readCapacityNow(raw: unknown): CapacityNow | null {
  if (!raw || typeof raw !== "object") return null;
  const v = raw as Record<string, unknown>;
  const transport = count(v.transport);
  const consumed = count(v.consumed);
  if (transport === null || consumed === null) return null;
  return {
    transport,
    consumed,
    fighters: count(v.fighters) ?? 0,
    unusedSupport: count(v.unused_support) ?? 0,
  };
}

export function readCapacityFacts(option: ChoiceOption): CapacityFacts | null {
  const p = option.payload ?? {};
  const role = p.capacity_role;
  if (role !== "fighter" && role !== "ground" && role !== "none") return null;
  const placement = p.space_placement;
  return {
    provided: count(p.capacity_provided) ?? 0,
    needed: count(p.capacity_needed) ?? 0,
    role,
    placement: placement === "always" || placement === "choice" ? placement : "never",
  };
}

/** The capacity position with `draft` (option id -> number of builds) staged. */
export function capacityMeter(
  now: CapacityNow,
  options: readonly ChoiceOption[],
  draft: Readonly<Record<string, number>>,
): CapacityMeter {
  let fromQueued = 0;
  let queuedFighters = 0;
  let queuedGround = 0;
  let maybeGround = 0;
  for (const option of options) {
    const builds = draft[option.id] ?? 0;
    const facts = builds > 0 ? readCapacityFacts(option) : null;
    if (!facts) continue;
    if (facts.placement === "always") {
      fromQueued += facts.provided * builds;
      if (facts.role === "fighter") queuedFighters += facts.needed * builds;
      if (facts.role === "ground") queuedGround += facts.needed * builds;
    } else if (facts.placement === "choice" && facts.role === "ground") {
      maybeGround += facts.needed * builds;
    }
  }
  // A space dock's fighter support excuses fighters and nothing else (16.3), so it is used up by
  // the staged fighters before they take capacity.
  const fightersTaking = Math.max(0, queuedFighters - now.unusedSupport);
  const consumed = now.consumed + fightersTaking + queuedGround;
  const transport = now.transport + fromQueued;
  const over = Math.max(0, consumed - transport);
  // "Fighters" only when no ground force is in play and the excess is within the fighters' share.
  const fighterShare = now.fighters + queuedFighters;
  const overKind = queuedGround === 0 && over <= fighterShare ? "fighters" : "units";
  return {
    existing: now.transport,
    fromQueued,
    transport,
    consumed,
    consumedNow: now.consumed,
    queuedFighters,
    queuedGround,
    maybeGround,
    over,
    overKind,
  };
}

export function capacityMeterText(meter: CapacityMeter): string {
  const base = `Capacity ${meter.consumed} / ${meter.transport}`;
  if (meter.over === 0) return base;
  const noun = meter.overKind === "fighters" ? (meter.over === 1 ? "fighter" : "fighters") : meter.over === 1 ? "unit" : "units";
  return `${base} (${meter.over} ${noun} over: they would be removed)`;
}
