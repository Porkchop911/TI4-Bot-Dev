import { isPublicEvent, visibilityKind } from "../protocol/eventVisibility.ts";
import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { GameLogEntry, HistoryChange } from "../protocol/client.ts";
import { CurrentLogPath } from "../protocol/types.ts";
import {
  useParticipantParts,
  useParticipantText,
  usePlayerIdentity,
} from "../presentation/PlayerIdentity.tsx";
import { ACTION_CARDS, getActionCardDescription } from "../protocol/contentCatalog.ts";
import {
  copyReplay,
  downloadText,
  replayCopyMessage,
  type ReplayCopyState,
} from "../presentation/replayCopy.ts";
import { followTarget, isPinned, nextUnseen, restoreTarget } from "./eventLogScroll.ts";

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
  /** Any seated player: loads the replay JSON so it can be copied out of the log drawer. */
  onFetchReplay?: () => Promise<{ text: string; filename: string }>;
  /** Turn redo: lets a seated player rewind their own last turn(s); the host may pick any seat. */
  turnRedo?: TurnRedoRequestControls;
  /** Single-player games: the newest entry comes first (rounds, phases, actions and events), and new entries appear at the top. */
  newestFirst?: boolean;
  /** Single-player undo: one button that takes back the viewer's last answer. */
  undoLast?: UndoLastControls;
}

export interface UndoLastControls {
  /** The engine can take the last answer back right now. */
  available: boolean;
  /** A history change is running. */
  busy: boolean;
  /** Resolves true when the answer was taken back. */
  onUndo: () => Promise<boolean>;
}

/** Tooltip on the undo/redo buttons while a history change is still running. */
export const HISTORY_BUSY_REASON = "A history change is still running; wait for it to finish";

export interface TurnRedoRequestControls {
  onRequest: (options: { turns?: 1 | 2; seat?: string }) => void;
  /** A request is running, or a redo is already in flight: the button waits. */
  disabled: boolean;
  /** Host only: the seats whose turn may be redone (the viewer's own seat first). */
  seats?: { id: string; label: string }[];
  viewerSeat?: string | null;
  /** A game on this device (single player): the bots just play on from the start of the turn. */
  local?: boolean;
}

export interface LogNode {
  id: string;
  kind: "round" | "phase" | "event" | "decision" | "marker" | "roll";
  label: string;
  children: LogNode[];
  entry?: GameLogEntry;
  count: number;
  actor?: string;
  stage?: string;
  eventIndex?: number;
  actionId?: string;
  actionType?: string;
  actionActor?: string;
}

const heading = (name: string) => name.replace(/_/g, " ").replace(/\b\w/g, (s) => s.toUpperCase());
const node = (id: string, kind: LogNode["kind"], label: string): LogNode => ({
  id,
  kind,
  label,
  children: [],
  count: 0,
});

/**
 * Build a map of action card names and their IDs for quick lookup.
 * Sorted by length (longest first) to avoid partial matches.
 */
const buildActionCardMap = () => {
  const entries = Object.entries(ACTION_CARDS)
    .map(([id, card]) => ({ id, name: (card as { name: string }).name }))
    .sort((a, b) => b.name.length - a.name.length);
  return entries;
};

const actionCardEntries = buildActionCardMap();

/**
 * Parse text for action card names and wrap them with tooltip spans.
 * Returns an array of strings and React elements.
 */
function parseActionCardsInText(text: string): (string | React.ReactElement)[] {
  if (!actionCardEntries.length) return [text];

  const parts: (string | React.ReactElement)[] = [];
  let remaining = text;
  let offset = 0;

  while (remaining.length > 0) {
    let found = false;

    for (const { id, name } of actionCardEntries) {
      const index = remaining.toLowerCase().indexOf(name.toLowerCase());
      if (index !== -1) {
        // Add text before the match
        if (index > 0) {
          parts.push(remaining.substring(0, index));
        }

        // Extract the actual matched text (preserving original case)
        const matchedText = remaining.substring(index, index + name.length);

        // Add the wrapped action card with tooltip
        const description = getActionCardDescription(id);
        parts.push(
          <span
            key={`action-card-${offset}-${index}`}
            className="event-log__action-card"
            title={description}
          >
            {matchedText}
          </span>,
        );

        // Move forward
        remaining = remaining.substring(index + name.length);
        offset += index + name.length;
        found = true;
        break;
      }
    }

    if (!found) {
      // No more action cards found, add remaining text
      parts.push(remaining);
      break;
    }
  }

  return parts.length ? parts : [text];
}

/** Preserve stream order with flattened structure. Each event gets unique eventIndex within phase. */
export function buildEventTree(events: readonly GameLogEntry[]): LogNode[] {
  const rounds: LogNode[] = [];
  const roundByKey = new Map<string, LogNode>();
  const phaseByKey = new Map<string, LogNode>();
  const decisions = new Map<string, LogNode>();
  let boundaryRound: number | undefined;
  let boundaryPhase: string | undefined;
  let lastActionId: string | undefined;
  let lastStage: string | undefined;

  // Track event indices per phase to ensure unique keys
  const phaseEventIndices = new Map<string, number>();

  for (const entry of events) {
    const event = entry.event;
    // The facts of the state and the changes of the board are for web2's Log: no line here.
    if (event.kind === "state_changed" || event.kind === "board_changed") {
      continue;
    }
    if (event.kind === "game_initialized" || event.kind === "phase_transition") {
      boundaryRound = event.round;
      boundaryPhase = event.phase;
    }
    const stamped = event.kind === "decision_resolved" || event.kind === "dice_rolled";
    const round = stamped ? (entry.round ?? boundaryRound) : boundaryRound;
    const phase = stamped ? (entry.phase ?? boundaryPhase) : boundaryPhase;
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
      phaseEventIndices.set(phaseKey, 0);
    }

    // Get and increment event index for this phase
    const eventIndex = phaseEventIndices.get(phaseKey)!;
    phaseEventIndices.set(phaseKey, eventIndex + 1);

    // A die roll is one line in the stream, where it happened; it does not start or end an
    // action or a stage, and there is nothing to undo to.
    if (event.kind === "dice_rolled") {
      phaseNode.children.push({
        ...node(`${phaseKey}:event:${eventIndex}`, "roll", ""),
        entry,
        actor: entry.actor,
        eventIndex,
      });
      continue;
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
      phaseNode.children.push({
        ...node(`${phaseKey}:event:${eventIndex}`, "marker", label),
        entry,
      });
      lastActionId = undefined;
      lastStage = undefined;
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

    // Insert action marker if this is a new action
    if (entry.action_id && entry.action_id !== lastActionId) {
      const actionEventIndex = phaseEventIndices.get(phaseKey)!;
      phaseEventIndices.set(phaseKey, actionEventIndex + 1);
      phaseNode.children.push({
        ...node(`${phaseKey}:event:${actionEventIndex}`, "marker",
                entry.action_type ? `${heading(entry.action_type)} action` : "Action"),
        actionId: entry.action_id,
        actionType: entry.action_type,
        actionActor: entry.action_actor,
      });
      lastActionId = entry.action_id;
      lastStage = undefined;
    }

    // Insert stage marker if this is a new stage
    if (entry.action_id || lastActionId) {
      const currentStage = entry.stage ?? "other";
      if (currentStage !== lastStage) {
        const stageEventIndex = phaseEventIndices.get(phaseKey)!;
        phaseEventIndices.set(phaseKey, stageEventIndex + 1);
        phaseNode.children.push({
          ...node(`${phaseKey}:event:${stageEventIndex}`, "marker",
                  currentStage === "other" ? "General" : heading(currentStage)),
          stage: currentStage,
        });
        lastStage = currentStage;
      }
    }

    // Add decision event
    const decisionEventIndex = phaseEventIndices.get(phaseKey)!;
    phaseEventIndices.set(phaseKey, decisionEventIndex + 1);
    const leaf = {
      ...node(`${phaseKey}:event:${decisionEventIndex}`, "decision", ""),
      entry,
      actor: entry.actor,
      eventIndex: decisionEventIndex,
    };
    phaseNode.children.push(leaf);
    decisions.set(key, leaf);
    roundNode.count++;
    phaseNode.count++;
  }
  return rounds;
}

const isHeading = (node: LogNode) => node.kind === "marker" && !node.entry;

/**
 * A phase's flat list as sections: an action or stage heading followed by its events. Newest
 * first the sections swap places and the events inside each swap too, but the heading stays on top.
 */
function reverseSections(children: readonly LogNode[]): LogNode[] {
  const sections: LogNode[][] = [];
  let current: LogNode[] = [];
  let hasBody = false;
  for (const child of children) {
    if (isHeading(child) && hasBody) {
      sections.push(current);
      current = [];
      hasBody = false;
    }
    current.push(child);
    if (!isHeading(child)) hasBody = true;
  }
  if (current.length) sections.push(current);
  return sections.reverse().flatMap((section) => [
    ...section.filter(isHeading),
    ...section.filter((node) => !isHeading(node)).reverse(),
  ]);
}

/** The log tree, newest first: latest round, its latest phase, the latest action and event at the top. */
export function newestFirstTree(tree: readonly LogNode[]): LogNode[] {
  return [...tree].reverse().map((round) => ({
    ...round,
    children: [...round.children].reverse().map((phase) => ({ ...phase, children: reverseSections(phase.children) })),
  }));
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
  // With flattened structure, we open the phase and it will show all nested events.
  // No need to track individual action/stage opening since they're not expandable.
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
  onFetchReplay,
  turnRedo,
  newestFirst = false,
  undoLast,
}) => {
  const [undoNote, setUndoNote] = useState("");
  const [undoRunning, setUndoRunning] = useState(false);
  const [redoTurns, setRedoTurns] = useState<1 | 2>(1);
  const [redoSeat, setRedoSeat] = useState<string | undefined>(undefined);
  const [replayState, setReplayState] = useState<ReplayCopyState>({ kind: "idle" });
  const copyReplayToClipboard = async () => {
    if (!onFetchReplay || replayState.kind === "busy") return;
    setReplayState({ kind: "busy" });
    setReplayState(
      await copyReplay({
        fetchReplay: onFetchReplay,
        writeClipboard: navigator.clipboard?.writeText
          ? (text) => navigator.clipboard.writeText(text)
          : undefined,
        download: downloadText,
      }),
    );
  };
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const parts = useParticipantParts();
  const tree = useMemo(() => buildEventTree(events), [events]);
  const shownTree = useMemo(() => (newestFirst ? newestFirstTree(tree) : tree), [tree, newestFirst]);
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
  // Scroll-follow: a reader at the bottom keeps following new events (and a newly opened round or
  // phase); a reader who scrolled up is never moved. The list starts pinned, so it opens at the
  // newest entry. The list unmounts while hidden, so the position is remembered and restored.
  const listRef = useRef<HTMLDivElement | null>(null);
  const pinned = useRef(true);
  const savedTop = useRef(0);
  const manualToggle = useRef(false);
  const lastLength = useRef(events.length);
  const [unseen, setUnseen] = useState(0);
  const measure = (el: HTMLDivElement) => ({
    scrollTop: el.scrollTop,
    scrollHeight: el.scrollHeight,
    clientHeight: el.clientHeight,
  });
  const onListScroll = (event: React.UIEvent<HTMLDivElement>) => {
    const el = event.currentTarget;
    pinned.current = isPinned(measure(el), newestFirst);
    savedTop.current = el.scrollTop;
    if (pinned.current) setUnseen(0);
  };
  const jumpToLatest = () => {
    const el = listRef.current;
    if (!el) return;
    pinned.current = true;
    el.scrollTop = followTarget(true, measure(el), newestFirst) ?? 0;
    setUnseen(0);
  };
  // Shown again: back to the bottom if the reader was pinned, else to where they were.
  useLayoutEffect(() => {
    const el = listRef.current;
    if (!isOpen || !el) return;
    el.scrollTop = restoreTarget(pinned.current, savedTop.current, measure(el), newestFirst);
  }, [isOpen]);
  // Content grew (new events, a new round/phase node opening, a new history generation).
  useLayoutEffect(() => {
    const el = listRef.current;
    const previous = lastLength.current;
    lastLength.current = events.length;
    if (!isOpen || !el) return;
    if (manualToggle.current) {
      // The reader's own expand/collapse: never move them, just re-read where they are.
      manualToggle.current = false;
      pinned.current = isPinned(measure(el), newestFirst);
      return;
    }
    const target = followTarget(pinned.current, measure(el), newestFirst);
    if (target !== undefined) el.scrollTop = target;
    else savedTop.current = el.scrollTop;
    setUnseen((old) => nextUnseen(old, pinned.current, previous, events.length));
  }, [tree, expanded, isOpen, events.length]);
  const toggle = (id: string) => {
    manualToggle.current = true;
    setManual((old) => new Set(old).add(id));
    setExpanded((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const renderNode = (item: LogNode, depth: number): React.ReactNode => {
    if (item.kind === "decision" || item.kind === "roll") {
      const entry = item.entry!;
      const roll = item.kind === "roll" ? entry.roll : undefined;
      const actor = entry.actor ? display(entry.actor) : null;
      const actorTitle = actor
        ? actor.position && !/\bposition \d+\b/i.test(actor.label)
          ? `${actor.label} · Position ${actor.position}`
          : actor.label
        : "Unknown participant";
      const eventNumber = entry.id.match(/-(\d+)$/)?.[1];
      const text =
        entry.private_detail ??
        entry.detail ??
        (entry.movement
          ? `${display(entry.movement.actor).label} moved ${entry.movement.unit} from #${entry.movement.origin} to #${entry.movement.destination}`
          : "Decision resolved");
      return (
        <div
          key={item.id}
          className={roll ? "event-log__entry event-log__entry--roll" : "event-log__entry"}
          data-testid={roll ? "event-log-roll" : "event-log-entry"}
          style={{ paddingLeft: depth * 14 }}
        >
          <span className="event-log__index">
            {eventNumber
              ? `#${eventNumber}`
              : entry.decision_count === undefined
                ? "·"
                : `#${entry.decision_count}`}
          </span>
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
          <span className="event-log__body">
            {parts(text).map((part, index) => {
              const partText = typeof part === "string" ? part : part.label.replace(/ \([●▲■◆★✚⬟◖] Position \d+\)$/, "");
              const actionCardParts = parseActionCardsInText(partText);

              return typeof part === "string" ? (
                <React.Fragment key={index}>
                  {actionCardParts}
                </React.Fragment>
              ) : (
                <span
                  key={index}
                  className="event-log__participant"
                  style={{ color: part.color }}
                >
                  {actionCardParts}
                </span>
              );
            })}
          </span>
          {roll && (
            <span className="event-log__dice" aria-label={`Dice: ${roll.faces.join(", ")}`}>
              {roll.faces.map((face, at) => (
                <span
                  key={at}
                  className="event-log__die"
                  data-hit={roll.hits_on === undefined ? undefined : face >= roll.hits_on ? "yes" : "no"}
                  data-reroll={roll.rerolled?.includes(at) ? "yes" : undefined}
                >
                  {face}
                </span>
              ))}
            </span>
          )}
          {!isPublicEvent(entry) && (
            <span className="event-log__private">
              {visibilityKind(entry.visibility) === "seat" ? "Private" : "Referee"}
            </span>
          )}
          {entry.timestamp && <time className="event-log__meta">{entry.timestamp}</time>}
          {entry.version !== undefined && <span className="event-log__meta">v{entry.version}</span>}
          {onRestore &&
            !roll &&
            entry.decision_count !== undefined &&
            entry.decision_count <= cursor && (
              <button
                type="button"
                className="event-log__undo"
                disabled={busy}
                title={busy ? HISTORY_BUSY_REASON : undefined}
                aria-label={`Undo from decision ${entry.decision_count}`}
                onClick={() => onRestore(entry.decision_count! - 1)}
              >
                Undo
              </button>
            )}
        </div>
      );
    }

    if (item.kind === "marker") {
      // Check if this is a phase/game marker (has entry) or action/stage marker (no entry)
      if (item.entry) {
        // Phase transition or game finished marker
        return (
          <div
            key={item.id}
            className="event-log__entry event-log__entry--marker"
            data-testid="event-log-entry"
            style={{ paddingLeft: depth * 14 }}
          >
            <span className="event-log__index">·</span>
            <span className="event-log__body">{present(item.label)}</span>
          </div>
        );
      } else {
        // Action/stage marker - render as visual separator with label
        const isAction = !!item.actionId;
        const isStage = !!item.stage;

        if (isAction) {
          const actionDisplay = display(item.actionActor ?? "unknown");
          return (
            <div
              key={item.id}
              className="event-log__action-marker"
              style={{ paddingLeft: depth * 14 }}
            >
              <span className="event-log__action-label">{item.label}</span>
              {item.actionActor && (
                <span
                  className="event-log__action-actor"
                  style={{ color: actionDisplay?.color ?? "#94a3b8" }}
                  title={actionDisplay?.label}
                >
                  · {actionDisplay?.label}
                </span>
              )}
            </div>
          );
        }

        if (isStage) {
          return (
            <div
              key={item.id}
              className="event-log__stage-marker"
              style={{ paddingLeft: depth * 14 }}
            >
              <span className="event-log__stage-label">{item.label}</span>
            </div>
          );
        }
      }
      return null;
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
          {unseen > 0 && (
            // Right under the toggle: on a phone the turn bar and drawer buttons cover the rest.
            <button
              type="button"
              className="button button--secondary button--sm event-log__jump"
              data-testid="event-log-jump"
              onClick={jumpToLatest}
            >
              {unseen} new {unseen === 1 ? "event" : "events"} {newestFirst ? "↑" : "↓"} Jump to latest
            </button>
          )}
          {undoLast && (
            <div className="event-log__undo-last" data-testid="event-log-undo-last-row">
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="event-log-undo-last"
                disabled={!undoLast.available || undoLast.busy || undoRunning}
                aria-describedby="event-log-undo-status"
                title={
                  undoLast.busy || undoRunning
                    ? HISTORY_BUSY_REASON
                    : undoLast.available
                      ? "Takes back your last decision. The other seats' moves after it are played again from the same seed."
                      : "Nothing to undo: your last decision cannot be taken back right now"
                }
                onClick={async () => {
                  setUndoRunning(true);
                  setUndoNote("Undoing your last decision…");
                  const done = await undoLast.onUndo().catch(() => false);
                  setUndoRunning(false);
                  setUndoNote(
                    done
                      ? "Undid your last decision. The game is back at the question before it."
                      : "Could not undo your last decision.",
                  );
                }}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="event-log__undo-icon">
                  <path d="M9 14L4 9l5-5M4 9h9a6 6 0 0 1 0 12h-4" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                Undo last decision
              </button>
              <span
                id="event-log-undo-status"
                role="status"
                aria-live="polite"
                data-testid="event-log-undo-status"
                className="event-log__replay-status"
              >
                {undoNote || (undoLast.available ? "" : "Nothing to undo yet.")}
              </span>
            </div>
          )}
          {onFetchReplay && (
            <div className="event-log__replay" data-testid="event-log-replay">
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="copy-replay-btn"
                disabled={replayState.kind === "busy"}
                onClick={() => void copyReplayToClipboard()}
              >
                {replayState.kind === "busy" ? "Copying…" : "Copy replay"}
              </button>
              <span
                role="status"
                aria-live="polite"
                data-testid="copy-replay-status"
                data-state={replayState.kind}
                className={
                  replayState.kind === "error"
                    ? "event-log__replay-status text-danger"
                    : "event-log__replay-status"
                }
              >
                {replayCopyMessage(replayState)}
              </span>
            </div>
          )}
          {turnRedo && (
            <div className="event-log__turn-redo" data-testid="event-log-turn-redo">
              {turnRedo.seats && turnRedo.seats.length > 1 && (
                <label className="event-log__turn-redo-field">
                  <span>Whose turn</span>
                  <select
                    data-testid="turn-redo-seat"
                    value={redoSeat ?? turnRedo.viewerSeat ?? turnRedo.seats[0].id}
                    onChange={(event) => setRedoSeat(event.target.value)}
                  >
                    {turnRedo.seats.map((seat) => (
                      <option key={seat.id} value={seat.id}>
                        {seat.label}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <label className="event-log__turn-redo-field">
                <span>Go back over</span>
                <select
                  data-testid="turn-redo-turns"
                  value={redoTurns}
                  onChange={(event) => setRedoTurns(event.target.value === "2" ? 2 : 1)}
                >
                  <option value="1">the last turn</option>
                  <option value="2">the last two turns</option>
                </select>
              </label>
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="turn-redo-btn"
                disabled={turnRedo.disabled}
                title={
                  turnRedo.local
                    ? "Takes the game back to the start of your turn so you can play it again. The bots decide again from there; the turn you took back can be redone with Redo."
                    : "Rewinds the game for everyone to the start of that turn. After the new turn, the recorded decisions that followed replay (same dice, same cards) until that seat's next turn begins. The original timeline is kept and can be restored."
                }
                onClick={() =>
                  turnRedo.onRequest({
                    turns: redoTurns,
                    seat:
                      redoSeat && redoSeat !== turnRedo.viewerSeat ? redoSeat : undefined,
                  })
                }
              >
                {turnRedo.local
                  ? "Undo my last turn"
                  : !redoSeat || redoSeat === turnRedo.viewerSeat
                  ? "Redo my last turn"
                  : "Redo their last turn"}
              </button>
            </div>
          )}
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
                  title={busy ? HISTORY_BUSY_REASON : undefined}
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
            ref={listRef}
            onScroll={onListScroll}
          >
            {shownTree.length ? (
              shownTree.map((item) => renderNode(item, 0))
            ) : (
              <div>No events recorded yet.</div>
            )}
          </div>
        </>
      )}
    </div>
  );
};
