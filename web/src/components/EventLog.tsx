import React, { useEffect, useMemo, useState } from "react";
import { GameLogEntry, HistoryChange } from "../protocol/client.ts";
import { CurrentLogPath } from "../protocol/types.ts";
import {
  useParticipantParts,
  useParticipantText,
  usePlayerIdentity,
} from "../presentation/PlayerIdentity.tsx";

export interface EventLogProps {
  events: GameLogEntry[];
  isOpen: boolean;
  onToggle: () => void;
  onRestore?: (cursor: number) => void;
  onChangeHistory?: (action: HistoryChange) => void;
  cursor?: number;
  redoCount?: number;
  busy?: boolean;
  currentPath?: CurrentLogPath;
  historyKey?: unknown;
}

export interface LogNode {
  id: string;
  kind: "round" | "phase" | "action" | "stage" | "decision" | "marker";
  label: string;
  children: LogNode[];
  entry?: GameLogEntry;
  count: number;
  actor?: string;
  stage?: string;
}

const heading = (name: string) => name.replace(/_/g, " ").replace(/\b\w/g, (s) => s.toUpperCase());
const node = (id: string, kind: LogNode["kind"], label: string): LogNode => ({
  id,
  kind,
  label,
  children: [],
  count: 0,
});

/** Preserve stream order, including repeated stage segments and same-cursor boundary events. */
export function buildEventTree(events: readonly GameLogEntry[]): LogNode[] {
  const rounds: LogNode[] = [];
  const roundByKey = new Map<string, LogNode>();
  const phaseByKey = new Map<string, LogNode>();
  const decisions = new Map<string, LogNode>();
  let boundaryRound: number | undefined;
  let boundaryPhase: string | undefined;
  let lastAction: LogNode | undefined;
  let lastStage: LogNode | undefined;
  let lastStageParent: LogNode | undefined;
  let lastParent: LogNode | undefined;
  for (const entry of events) {
    const event = entry.event;
    if (event.kind === "game_initialized" || event.kind === "phase_transition") {
      boundaryRound = event.round;
      boundaryPhase = event.phase;
    }
    const round =
      event.kind === "decision_resolved" ? (entry.round ?? boundaryRound) : boundaryRound;
    const phase =
      event.kind === "decision_resolved" ? (entry.phase ?? boundaryPhase) : boundaryPhase;
    const roundKey = round === undefined ? "unknown" : String(round);
    let roundNode = roundByKey.get(roundKey);
    if (!roundNode) {
      roundNode = node(
        `round:${roundKey}`,
        "round",
        round === undefined ? "Unknown round" : `Round ${round}`,
      );
      rounds.push(roundNode);
      roundByKey.set(roundKey, roundNode);
    }
    const phaseKey = `${roundNode.id}:${phase ?? "unknown"}`;
    let phaseNode = phaseByKey.get(phaseKey);
    if (!phaseNode) {
      phaseNode = node(phaseKey, "phase", phase ? `${heading(phase)} phase` : "Unknown phase");
      roundNode.children.push(phaseNode);
      phaseByKey.set(phaseKey, phaseNode);
    }
    if (event.kind !== "decision_resolved") {
      const label =
        event.kind === "game_initialized"
          ? "Game initialized"
          : event.kind === "phase_transition"
            ? "Phase began"
            : event.winner
              ? `Game finished: ${event.winner} wins`
              : "Game finished: draw";
      phaseNode.children.push({ ...node(`marker:${entry.id}`, "marker", label), entry });
      lastAction = lastStage = lastStageParent = lastParent = undefined;
      continue;
    }
    const key =
      entry.decision_count === undefined
        ? entry.id
        : `${phaseNode.id}:cursor:${entry.decision_count}`;
    const existing = decisions.get(key);
    if (existing) {
      const prior = existing.entry!;
      existing.entry = {
        ...prior,
        detail: prior.detail ?? entry.detail,
        movement: prior.movement ?? entry.movement,
        private_detail: entry.private_detail ?? prior.private_detail,
        actor: prior.actor ?? entry.actor,
      };
      continue;
    }
    let parent = phaseNode;
    if (entry.action_id) {
      const id = `${phaseNode.id}:action:${entry.action_id}`;
      if (lastAction?.id !== id || lastParent !== phaseNode) {
        lastAction = node(
          id,
          "action",
          entry.action_type ? `${heading(entry.action_type)} action` : "Action",
        );
        lastAction.actor = entry.action_actor;
        phaseNode.children.push(lastAction);
        lastStage = undefined;
        lastStageParent = undefined;
      }
      parent = lastAction;
    } else {
      lastAction = lastStage = lastStageParent = undefined;
      // Older events have no action ID. Keep them under their phase rather than
      // implying that each decision belongs to a separate, unknown action.
    }
    lastParent = phaseNode;
    const actionNode = parent.kind === "action" ? parent : undefined;
    if (entry.action_id || parent.kind === "action") {
      const stage = entry.stage ?? "other";
      if (lastStage?.stage !== stage || lastStageParent !== parent) {
        lastStage = node(
          `${parent.id}:stage:${stage}:${entry.id}`,
          "stage",
          stage === "other" ? "General" : heading(stage),
        );
        lastStage.stage = stage;
        parent.children.push(lastStage);
        lastStageParent = parent;
      }
      parent = lastStage;
    }
    const leaf = { ...node(`decision:${key}`, "decision", ""), entry, actor: entry.actor };
    parent.children.push(leaf);
    decisions.set(key, leaf);
    roundNode.count++;
    phaseNode.count++;
    if (actionNode) actionNode.count++;
    if (parent.kind === "stage") parent.count++;
  }
  return rounds;
}

function openPath(tree: LogNode[], path?: CurrentLogPath): Set<string> {
  const opened = new Set<string>();
  if (!path) return opened;
  const round = tree.find((n) => n.id === `round:${path.round}`);
  if (!round) return opened;
  opened.add(round.id);
  const phase = round.children.find((n) => n.id === `${round.id}:${path.phase}`);
  if (!phase) return opened;
  opened.add(phase.id);
  const action = phase.children.find((n) => n.id === `${phase.id}:action:${path.action_id}`);
  if (!path.action_id || !action) return opened;
  opened.add(action.id);
  const stage = path.stage
    ? [...action.children].reverse().find((n) => n.kind === "stage" && n.stage === path.stage)
    : [...action.children].reverse().find((n) => n.kind === "stage");
  if (stage) opened.add(stage.id);
  return opened;
}

export const EventLog: React.FC<EventLogProps> = ({
  events,
  isOpen,
  onToggle,
  onRestore,
  onChangeHistory,
  cursor = 0,
  redoCount = 0,
  busy = false,
  currentPath,
  historyKey,
}) => {
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const parts = useParticipantParts();
  const tree = useMemo(() => buildEventTree(events), [events]);
  const [expanded, setExpanded] = useState<Set<string>>(() => openPath(tree, currentPath));
  const [manual, setManual] = useState<Set<string>>(() => new Set());
  useEffect(() => {
    setExpanded(openPath(tree, currentPath));
    setManual(new Set());
    // Only a new snapshot/history generation resets the reader's navigation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [historyKey]);
  useEffect(() => {
    if (!currentPath) return;
    setExpanded((old) => {
      const next = new Set(old);
      for (const id of openPath(tree, currentPath)) if (!manual.has(id)) next.add(id);
      return next;
    });
  }, [
    currentPath?.round,
    currentPath?.phase,
    currentPath?.action_id,
    currentPath?.stage,
    tree,
    manual,
  ]);
  const toggle = (id: string) => {
    setManual((old) => new Set(old).add(id));
    setExpanded((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const renderNode = (item: LogNode, depth: number): React.ReactNode => {
    if (item.kind === "decision" || item.kind === "marker") {
      const entry = item.entry!;
      const actor = entry.actor ? display(entry.actor) : null;
      const actorTitle = actor
        ? actor.position && !/\bposition \d+\b/i.test(actor.label)
          ? `${actor.label} · Position ${actor.position}`
          : actor.label
        : "Unknown participant";
      const eventNumber = entry.id.match(/-(\d+)$/)?.[1];
      const text =
        item.kind === "marker"
          ? item.label
          : (entry.private_detail ??
            entry.detail ??
            (entry.movement
              ? `${display(entry.movement.actor).label} moved ${entry.movement.unit} from #${entry.movement.origin} to #${entry.movement.destination}`
              : "Decision resolved"));
      return (
        <div
          key={item.id}
          className="event-log__entry"
          data-testid="event-log-entry"
          style={{ paddingLeft: depth * 14 }}
        >
          <span className="event-log__index">
            {eventNumber
              ? `#${eventNumber}`
              : entry.decision_count === undefined
                ? "·"
                : `#${entry.decision_count}`}
          </span>
          {item.kind === "decision" && (
            <span
              className="event-log__actor"
              tabIndex={0}
              title={actorTitle}
              data-tooltip={actor?.label ?? "Unknown participant"}
              aria-label={actorTitle}
              style={{ color: actor?.color ?? "#94a3b8" }}
            >
              {actor?.symbol ?? "?"}
            </span>
          )}
          <span className="event-log__body">
            {item.kind === "marker"
              ? present(text)
              : parts(text).map((part, index) =>
                  typeof part === "string" ? (
                    part
                  ) : (
                    <span
                      key={index}
                      className="event-log__participant"
                      style={{ color: part.color }}
                    >
                      {part.label.replace(/ \([●▲■◆★✚⬟◖] Position \d+\)$/, "")}
                    </span>
                  ),
                )}
          </span>
          {entry.visibility !== "public" && (
            <span className="event-log__private">
              {entry.visibility === "seat" ? "Private" : "Referee"}
            </span>
          )}
          {entry.timestamp && <time className="event-log__meta">{entry.timestamp}</time>}
          {entry.version !== undefined && <span className="event-log__meta">v{entry.version}</span>}
          {item.kind === "decision" &&
            onRestore &&
            entry.decision_count !== undefined &&
            entry.decision_count <= cursor && (
              <button
                type="button"
                className="event-log__undo"
                disabled={busy}
                aria-label={`Undo from decision ${entry.decision_count}`}
                onClick={() => onRestore(entry.decision_count! - 1)}
              >
                Undo
              </button>
            )}
        </div>
      );
    }
    const open = expanded.has(item.id);
    return (
      <div key={item.id} className={`event-log__group event-log__group--${item.kind}`}>
        <button
          type="button"
          className="event-log__heading"
          style={{ paddingLeft: depth * 14 }}
          aria-expanded={open}
          onClick={() => toggle(item.id)}
        >
          <span className="event-log__chevron">{open ? "▾" : "▸"}</span>
          {item.label}
          <span className="event-log__count">{item.count}</span>
          {item.actor && <span className="event-log__owner">· {display(item.actor).label}</span>}
        </button>
        {open && item.children.map((child) => renderNode(child, depth + 1))}
      </div>
    );
  };
  return (
    <div data-testid="event-log-container" className="event-log">
      <button
        id="event-log-toggle"
        type="button"
        data-testid="event-log-toggle"
        onClick={onToggle}
        aria-expanded={isOpen}
        aria-controls="event-log-list"
        className="button event-log__toggle"
      >
        <span>
          Event Log <span className="event-log__count">{events.length}</span>
        </span>
        <span>{isOpen ? "▾ Hide" : "▴ Show"}</span>
      </button>
      {isOpen && (
        <>
          {onChangeHistory && redoCount > 0 && (
            <div className="event-log__redo" aria-label="Redo history">
              <span>
                {redoCount} undone {redoCount === 1 ? "decision" : "decisions"}
              </span>
              {(["redo", "redo_batch", "redo_pipeline"] as const).map((action, index) => (
                <button
                  key={action}
                  type="button"
                  className="button button--secondary button--sm"
                  disabled={busy}
                  onClick={() => onChangeHistory(action)}
                >
                  Redo {["one", "batch", "action"][index]}
                </button>
              ))}
            </div>
          )}
          <div
            id="event-log-list"
            role="region"
            aria-labelledby="event-log-toggle"
            data-testid="event-log-list"
            className="event-log__list"
          >
            {tree.length ? (
              tree.map((item) => renderNode(item, 0))
            ) : (
              <div>No events recorded yet.</div>
            )}
          </div>
        </>
      )}
    </div>
  );
};
