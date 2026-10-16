import type { BoardView, InvasionStepView, PlacedUnitView } from "../protocol/types.ts";
import type { UnitCount } from "./combatSummary.ts";
import { countBy } from "./combatSummary.ts";
import { summarizeGroundCombat } from "./groundCombatSummary.ts";
import {
  formatCounts,
  planetName,
  stepOutcomeText,
  summarizeInvasionStep,
  type InvasionStepSummary,
} from "./invasionSteps.ts";

/** Structures are never ground forces: they are neither landed nor lost. */
const STRUCTURES = new Set(["pds", "space_dock", "spacedock"]);

/** Maps a seat id to the name shown for it. */
export type SeatName = (seat: string) => string;

/** A corner toast about an invasion step or the whole invasion. Seat names come from the caller. */
export interface InvasionToast {
  id: string;
  kind: "bombardment" | "space_cannon_defense" | "summary";
  /** The seat the toast is coloured for: the invader. */
  actor: string;
  /** Plain text; lines are separated by "\n". */
  text: string;
}

interface PlanetLog {
  /** The controller when the invasion was first seen. */
  controllerAtStart: string | null;
  /** Most ground forces of each player on the planet at any moment seen, per unit type. */
  peak: Map<string, Map<string, number>>;
  bombardment: InvasionStepSummary | null;
  spaceCannon: InvasionStepSummary | null;
  groundRounds: number;
  verdict: ReturnType<typeof summarizeGroundCombat>;
}

/** What is known of the invasion being watched, between two board updates. */
export interface InvasionTracker {
  seq: number;
  system: string;
  invader: string;
  planets: Map<string, PlanetLog>;
  announced: Set<string>;
  /** Seen first when it was already under way (page load, undo): no summary, as the start is missing. */
  partial: boolean;
}

export interface InvasionTrackResult {
  tracker: InvasionTracker | null;
  toasts: InvasionToast[];
}

const stepKey = (seq: number, step: InvasionStepView) =>
  `${seq}:${step.kind}:${step.planet}:${step.dice.map((d) => `${d.player}${d.face}`).join(",")}:${step.after.length}`;

function newLog(controller: string | null): PlanetLog {
  return {
    controllerAtStart: controller,
    peak: new Map(),
    bombardment: null,
    spaceCannon: null,
    groundRounds: 0,
    verdict: null,
  };
}

function observe(log: PlanetLog, units: readonly PlacedUnitView[]) {
  const bySeat = new Map<string, PlacedUnitView[]>();
  for (const u of units) {
    if (STRUCTURES.has(u.unit_type.toLowerCase())) continue;
    bySeat.set(u.owner, [...(bySeat.get(u.owner) ?? []), u]);
  }
  for (const [seat, own] of bySeat) {
    const peak = log.peak.get(seat) ?? new Map<string, number>();
    for (const [unit, count] of countBy(own)) peak.set(unit, Math.max(peak.get(unit) ?? 0, count));
    log.peak.set(seat, peak);
  }
}

const onPlanet = (board: BoardView, system: string, planet: string) =>
  (board.systems[system]?.units ?? []).filter((u) => u.planet === planet);

/**
 * Follows the invasion through successive boards: returns a toast when a bombardment or Space
 * Cannon Defense step shows up in `last_step`, and a summary toast when the invasion is over.
 * `quiet` is true for the first board a client sees (page load, after an undo): an invasion that
 * is already under way is followed without toasts, because its earlier steps are history.
 */
export function trackInvasion(
  previous: InvasionTracker | null,
  board: BoardView,
  quiet = false,
  name: SeatName = (seat) => seat,
): InvasionTrackResult {
  const invasion = board.invasion ?? null;
  if (!invasion) {
    if (!previous) return { tracker: null, toasts: [] };
    return { tracker: null, toasts: previous.partial ? [] : [summaryToast(previous, board, name)] };
  }
  let tracker = previous;
  const toasts: InvasionToast[] = [];
  let finished: InvasionToast[] = [];
  let suppress = false;
  if (!tracker || tracker.seq !== invasion.invasion_seq || tracker.system !== invasion.system_id) {
    // A different invasion began without the old one ending on a board we saw.
    if (tracker && !tracker.partial) finished = [summaryToast(tracker, board, name)];
    tracker = {
      seq: invasion.invasion_seq,
      system: invasion.system_id,
      invader: invasion.invader,
      planets: new Map(),
      announced: new Set(),
      partial: quiet,
    };
    suppress = quiet;
  }
  for (const planet of invasion.planets) {
    let log = tracker.planets.get(planet);
    if (!log) {
      log = newLog(board.systems[invasion.system_id]?.planets[planet]?.controlled_by ?? null);
      tracker.planets.set(planet, log);
    }
    observe(log, onPlanet(board, invasion.system_id, planet));
  }
  const step = invasion.last_step;
  if (step) {
    const log = tracker.planets.get(step.planet) ?? newLog(null);
    tracker.planets.set(step.planet, log);
    observe(log, step.before);
    observe(log, step.after);
    const key = stepKey(tracker.seq, step);
    if (!tracker.announced.has(key)) {
      tracker.announced.add(key);
      const summary = summarizeInvasionStep(step, tracker.invader);
      if (summary) {
        if (summary.kind === "bombardment") log.bombardment = summary;
        else log.spaceCannon = summary;
        if (!suppress)
          toasts.push({
            id: `invasion:${key}`,
            kind: summary.kind,
            actor: tracker.invader,
            text: stepOutcomeText(summary, tracker.invader, name),
          });
      } else if (step.kind === "ground_round") {
        log.groundRounds = Math.max(log.groundRounds, step.round);
        log.verdict = summarizeGroundCombat(step, tracker.invader) ?? log.verdict;
      }
    }
  }
  // After the first board the invasion is followed in full, even if it began half-seen.
  return { tracker, toasts: [...finished, ...toasts] };
}

const lossText = (peak: Map<string, number> | undefined, final: Map<string, number>): UnitCount[] => {
  const lost: UnitCount[] = [];
  for (const [unit, count] of peak ?? []) {
    const gone = count - (final.get(unit) ?? 0);
    if (gone > 0) lost.push({ unit, count: gone });
  }
  return lost;
};

/** The closing toast of an invasion: who invaded, what landed, what was lost and who holds each planet. */
export function summaryToast(
  tracker: InvasionTracker,
  board: BoardView,
  name: SeatName = (seat) => seat,
): InvasionToast {
  const lines: string[] = [];
  const planetNames = [...tracker.planets.keys()].map(planetName);
  lines.push(`Invasion of ${planetNames.join(" and ") || "the system"} by ${name(tracker.invader)}`);
  for (const [planet, log] of tracker.planets) {
    const final = onPlanet(board, tracker.system, planet).filter((u) => !STRUCTURES.has(u.unit_type.toLowerCase()));
    const finalBySeat = (seat: string) => countBy(final.filter((u) => u.owner === seat));
    const place = planetName(planet);
    const landed = [...(log.peak.get(tracker.invader) ?? new Map<string, number>())].map(([unit, count]) => ({ unit, count }));
    const mine = lossText(log.peak.get(tracker.invader), finalBySeat(tracker.invader));
    const parts: string[] = [];
    parts.push(landed.length ? `landed ${formatCounts(landed)}` : "no ground forces landed");
    if (landed.length) parts.push(mine.length ? `lost ${formatCounts(mine)}` : "lost nothing");
    for (const [seat, peak] of log.peak) {
      if (seat === tracker.invader) continue;
      const lost = lossText(peak, finalBySeat(seat));
      parts.push(`${name(seat)} lost ${lost.length ? formatCounts(lost) : "nothing"}`);
    }
    const sources: string[] = [];
    if (log.bombardment) sources.push(`bombardment ${log.bombardment.hits} hit${log.bombardment.hits === 1 ? "" : "s"}`);
    if (log.spaceCannon) sources.push(`Space Cannon ${log.spaceCannon.hits} hit${log.spaceCannon.hits === 1 ? "" : "s"}`);
    if (sources.length) parts.push(`(${sources.join(", ")})`);
    const controller = board.systems[tracker.system]?.planets[planet]?.controlled_by ?? null;
    let ground: string;
    if (log.verdict?.verdict.kind === "attacker_won") ground = `ground combat won after ${log.groundRounds} round${log.groundRounds === 1 ? "" : "s"}`;
    else if (log.verdict?.verdict.kind === "defender_held") ground = `ground combat lost after ${log.groundRounds} round${log.groundRounds === 1 ? "" : "s"}`;
    else if (log.verdict?.verdict.kind === "mutual_destruction") ground = "ground combat: both sides wiped out";
    else if (log.groundRounds > 0) ground = `ground combat ended after ${log.groundRounds} round${log.groundRounds === 1 ? "" : "s"}`;
    else ground = "no ground combat";
    const control = controller
      ? controller === log.controllerAtStart
        ? `${name(controller)} keeps ${place}`
        : `${name(controller)} now controls ${place}`
      : `${place} is uncontrolled`;
    lines.push(`${place}: ${parts.join("; ")}; ${ground}. ${control}.`);
  }
  return {
    id: `invasion:${tracker.seq}:${tracker.system}:summary`,
    kind: "summary",
    actor: tracker.invader,
    text: lines.join("\n"),
  };
}
