import { isPublicEvent } from "../protocol/eventVisibility.ts";
import type { GameEvent } from "../protocol/types.ts";

/**
 * One closing recap of another player's action ("tactical action in system 27: moved 3 ships, ...").
 *
 * Turn model. The server stamps every public `decision_resolved` entry of the action phase with an
 * `action_id`, which starts at the actor's `action phase` menu choice (stage "action selection",
 * with `action_type` tactical / strategic / component / pass) and stays the same for everything
 * that follows: activation, movement, combat, production, strategy card steps and the other seats'
 * reactions. The recap covers one such action. It is closed (the turn "ended") by the first of:
 *  - a pass: the selection itself is the whole turn;
 *  - the next entry with a different `action_id` (the next player picked their action);
 *  - a public `phase_transition` or `game_finished` entry;
 *  - the viewer being asked for their own action phase menu (the host passes this in), because the
 *    bare "end turn" decision leaves no public entry to detect.
 *
 * Only public entries are read, and only the same facts the public event log already shows
 * (`detail`, `action_type`, `stage`). Entries redacted to a seat or the referee, and public entries
 * without a detail, contribute nothing.
 */

export interface OpenTurn {
  actionId: string;
  actor: string;
  entries: GameEvent[];
}

export interface TurnRecap {
  /** Stable per action, so a replayed entry cannot create a second toast. */
  id: string;
  actor: string;
  /** Text after the actor's name. */
  text: string;
}

const isPublicDecision = (e: GameEvent) =>
  isPublicEvent(e) && e.event.kind === "decision_resolved";

const isSelection = (e: GameEvent) => e.stage === "action selection" && Boolean(e.action_type);

export interface TrackResult {
  open: OpenTurn | null;
  /** Turns that ended, oldest first. */
  closed: OpenTurn[];
  /** Ids of the entries that belong to a tracked turn (the recap will cover them). */
  tracked: Set<string>;
}

/**
 * Feeds new entries to the open turn. Turns of the viewer are never opened, and a turn whose
 * selection entry was never seen (the game was loaded mid-turn) is never recapped.
 */
export function trackTurns(
  open: OpenTurn | null,
  fresh: readonly GameEvent[],
  viewerSeat?: string | null,
): TrackResult {
  const closed: OpenTurn[] = [];
  const tracked = new Set<string>();
  let current: OpenTurn | null = open ? { ...open, entries: [...open.entries] } : null;
  const close = () => {
    if (current) closed.push(current);
    current = null;
  };
  for (const e of fresh) {
    if (isPublicEvent(e) && e.event.kind !== "decision_resolved") {
      // Phase change or game end: nothing continues.
      if (e.event.kind === "phase_transition" || e.event.kind === "game_finished") close();
      continue;
    }
    if (!isPublicDecision(e) || !e.action_id) continue;
    if (current && current.actionId !== e.action_id) close();
    if (current) {
      current.entries.push(e);
      tracked.add(e.id);
      continue;
    }
    const actor = e.action_actor ?? e.actor;
    if (!isSelection(e) || !actor || actor === viewerSeat) continue;
    current = { actionId: e.action_id, actor, entries: [e] };
    tracked.add(e.id);
    if (e.action_type === "pass") close();
  }
  return { open: current, closed, tracked };
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

const MAX_PARTS = 4;

/** "<action> in system N: moved 3 ships, ..." for one finished turn; null without a selection. */
export function buildTurnRecap(turn: OpenTurn): TurnRecap | null {
  const selection = turn.entries.find(isSelection);
  if (!selection) return null;
  const { actor } = turn;

  let system: string | null = null;
  let moved = 0;
  let landed = 0;
  let lost = 0;
  let destroyed = 0;
  let retreated = false;
  let fought = false;
  let produced = 0;
  let tokens = 0;
  const extras: string[] = [];

  for (const e of turn.entries) {
    if (!isPublicDecision(e)) continue;
    if (e.stage === "combat" || e.stage === "invasion") fought = true;
    const detail = e.detail?.trim();
    const who = e.actor;
    if (!detail || !who || !detail.startsWith(`${who} `)) continue;
    const text = detail.slice(who.length + 1);
    // Losses count for whoever suffers them; the turn's actor loses own, the rest are destroyed.
    if (/^lost an? /.test(text)) {
      if (who === actor) lost++;
      else destroyed++;
      continue;
    }
    if (who !== actor) continue;
    let m: RegExpExecArray | null;
    if ((m = /^activated #(\S+)$/.exec(text))) system ??= m[1];
    else if (/^moved .+ from #\S+ to #\S+$/.test(text)) moved++;
    else if (/^landed /.test(text)) landed++;
    else if (/^retreated from /.test(text)) retreated = true;
    else if ((m = /^produced (\d+) /.exec(text))) produced += Number(m[1]);
    else if (/^gained an? \w+ command token$/.test(text)) tokens++;
    else if ((m = /^researched (.+)$/.exec(text))) extras.push(`researched ${m[1]}`);
    else if ((m = /^scored (.+)$/.exec(text))) extras.push(`scored ${m[1]}`);
    else if ((m = /^placed (.+) on (.+)$/.exec(text))) extras.push(`placed ${m[1]} on ${m[2]}`);
    else if ((m = /^played (.+)$/.exec(text))) extras.push(`played ${m[1]}`);
    else if ((m = /^chose (.+) as speaker$/.exec(text))) extras.push(`chose ${m[1]} as speaker`);
  }

  const parts: string[] = [];
  if (moved) parts.push(`moved ${plural(moved, "ship")}`);
  if (landed) parts.push(`landed ${plural(landed, "unit")}`);
  if (fought || lost || destroyed) {
    // The log names losses, not winners, so the recap does too.
    const losses = [lost && `lost ${lost}`, destroyed && `destroyed ${destroyed}`].filter(Boolean);
    parts.push(
      `${retreated ? "retreated from combat" : "fought"}${losses.length ? ` (${losses.join(", ")})` : ""}`,
    );
  }
  if (produced) parts.push(`built ${plural(produced, "unit")}`);
  if (tokens) parts.push(`gained ${plural(tokens, "command token")}`);
  parts.push(...new Set(extras));

  const shown = parts.slice(0, MAX_PARTS).join(", ");
  const more = parts.length > MAX_PARTS ? `, and ${parts.length - MAX_PARTS} more` : "";
  const id = `recap:${turn.actionId}`;

  const kind = selection.action_type;
  if (kind === "pass") return { id, actor, text: "passed" };
  const label =
    kind === "tactical"
      ? `tactical action${system ? ` in system ${system}` : ""}`
      : kind === "strategic"
        ? "strategic action"
        : "component action";
  return { id, actor, text: parts.length ? `${label}: ${shown}${more}` : label };
}
