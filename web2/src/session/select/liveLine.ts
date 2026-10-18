// What another seat does now: the line over the board, and the step of the live row of the Log.
import type { LiveLineView, LogItemView, LogView } from "../../model";
import type { Catalog, SessionUpdate } from "../wire";
import { isGroundForce, isShip, seatLabel, unitType } from "./names";
import { planetNames } from "./payment";

/** The step of the live action as the board says it: "Space combat · round 2", "3 vs 2 ships". */
function fight(update: SessionUpdate): { step: string; forces: string } | null {
  const board = update.view.board;
  const combat = board.combat;
  if (combat) {
    const ships = (seat: string) =>
      (board.systems[combat.system_id]?.units ?? []).filter((unit) => {
        const type = unitType(unit.unit_type);
        return unit.owner === seat && unit.planet === null && type !== null && isShip(type);
      }).length;
    const step = combat.round > 0 ? `Space combat · round ${combat.round}` : "Space combat";
    return { step, forces: `${ships(combat.attacker)} vs ${ships(combat.defender)} ships` };
  }
  const invasion = board.invasion;
  if (invasion?.current_planet && invasion.defender && invasion.ground_round > 0) {
    const planet = invasion.current_planet;
    const forces = (seat: string) =>
      (board.systems[invasion.system_id]?.units ?? []).filter((unit) => {
        const type = unitType(unit.unit_type);
        return (
          unit.owner === seat && unit.planet === planet && type !== null && isGroundForce(type)
        );
      }).length;
    const name = planetNames(update).get(planet) ?? planet;
    return {
      step: `Ground combat · ${name} · round ${invasion.ground_round}`,
      forces: `${forces(invasion.invader)} vs ${forces(invasion.defender)} ground forces`,
    };
  }
  return null;
}

/**
 * Whom the action waits for, when it is not the seat that plays it: "Waiting for Hacan:
 * casualties" while hits are queued against that seat, "Waiting for reactions" in a window.
 */
function waiting(update: SessionUpdate, actor: string | null, printed: Catalog | null) {
  const status = update.turn_status;
  if (status.kind === "waiting_for_reactions") {
    return "Waiting for reactions";
  }
  if (status.kind !== "waiting_for_decision" || status.seat === actor) {
    return null;
  }
  const hits = update.view.board.combat?.remaining_hits[status.seat] ?? 0;
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const who = status.seat === viewer ? "you" : seatLabel(update, status.seat, printed);
  return `Waiting for ${who}${hits > 0 ? ": casualties" : ""}`;
}

/** The live entry of the Log: the turn or the phase that is played now. */
export const liveEntry = (log: LogView): LogItemView | null =>
  log.entries.findLast((entry) => entry.live) ?? null;

/**
 * What is played now, in words, for the live row of the Log and the line over the board.
 * `step` is the step of the action that is played now (the last tab of a tactical action that the
 * game came to); the board adds a combat under way.
 */
export function selectNow(
  update: SessionUpdate,
  entry: LogItemView,
  step: string | null,
  printed: Catalog | null,
): { step: string | null; detail: string | null } {
  const actor = entry.kind === "turn" ? entry.seat : null;
  const fighting = entry.kind === "turn" ? fight(update) : null;
  const now = fighting?.step ?? step;
  const wait = waiting(update, actor, printed);
  return { step: now, detail: wait ?? fighting?.forces ?? null };
}

/**
 * The line over the board while another seat acts: the seat, the action and where, the step and
 * whom it waits for. None while the viewer plays its own turn, and none while the game waits for a
 * decision of the viewer (a secondary of the card that is played, a window, a phase that asks the
 * viewer): then that decision is what the action waits for, and its sheet is the one place.
 */
export function selectLiveLine(
  update: SessionUpdate,
  entry: LogItemView | null,
  now: { step: string | null; detail: string | null },
  asked: boolean,
  printed: Catalog | null,
): LiveLineView | null {
  if (!entry || asked) {
    return null;
  }
  const parts =
    entry.kind === "turn"
      ? entry.mine
        ? null
        : [seatLabel(update, entry.seat, printed), entry.label, now.step, now.detail]
      : [entry.label.split(" · ")[0], now.detail];
  if (!parts) {
    return null;
  }
  return { turn: entry.id, text: parts.filter(Boolean).join(" · ") };
}
