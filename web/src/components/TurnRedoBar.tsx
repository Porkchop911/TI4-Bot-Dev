import React, { useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useCompactLayout, useWideLayout } from "../hooks/useCompactLayout.ts";
import type { TurnRedoBusy } from "../hooks/useTurnRedo.ts";
import {
  findActionCardMeta,
  findExplorationCardMeta,
  findPublicObjectiveMeta,
  findSecretObjectiveMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";
import type { TurnRedoConflict, TurnRedoConflictKind, TurnRedoStatus } from "../protocol/turnRedo.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { useBoardPrepSlot } from "../presentation/BoardPrepSlot.tsx";

/** A detail text longer than this is clamped on a phone, with a "Details" button for the rest. */
export const LONG_DETAIL_CHARS = 110;

export interface TurnRedoBarProps {
  status: TurnRedoStatus | null;
  busy: TurnRedoBusy;
  error: string | null;
  onAutoplay: () => void;
  onRestore: () => void;
  onKeep: () => void;
}

/** What stopped the replay, in words that name no hidden card and no recorded choice. */
const REASON: Record<TurnRedoConflictKind, string> = {
  actor: "the game now asks a different player",
  prompt: "the game now asks a different question",
  context: "the situation is different now",
  chosen_not_offered: "the recorded answer is not available any more",
  options_changed: "the options on offer are different now",
  quantity_changed: "the amounts owed or available are different now",
  engine_ended: "the game ended before the recorded decisions did",
  engine_error: "the game could not continue with the recorded decisions",
  reserved_card: "a card that was drawn for a recorded decision is no longer in the deck",
};

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

const DECK_NOUN: Record<string, string> = {
  action_card: "action card",
  secret: "secret objective",
  objective: "public objective",
  relic: "relic",
  agenda: "agenda",
};

/** "the action card Sabotage" / "an action card": the card is absent for a hidden deck's draw. */
function reservedCardText(deck: string | null, card: string | null): string {
  const kind = deck?.startsWith("exploration:")
    ? `${deck.slice("exploration:".length)} exploration card`
    : ((deck && DECK_NOUN[deck]) ?? "card");
  if (!card) return `an ${kind}`.replace(/^an ([^aeiou])/, "a $1");
  const meta =
    deck === "action_card"
      ? findActionCardMeta(card)
      : deck === "secret"
        ? findSecretObjectiveMeta(card)
        : deck === "objective"
          ? findPublicObjectiveMeta(card)
          : deck?.startsWith("exploration:")
            ? findExplorationCardMeta(card)
            : undefined;
  return `the ${kind} ${meta?.name ?? humanizeId(card)}`;
}

/** Why the replay stopped at a conflict, as one clause. */
function conflictReason(
  conflict: TurnRedoConflict,
  name: (seat: string) => string,
): string {
  if (conflict.kind === "reserved_card") {
    const who = name(conflict.recipient ?? conflict.seat);
    return `${reservedCardText(conflict.deck, conflict.card)} reserved for ${who} is no longer in the deck`;
  }
  return REASON[conflict.kind];
}

/**
 * The phone sheet: docked above the bottom bars, and told where the strip ends so its height can
 * stop short of the toolbar whenever there is room (see `.turn-redo__sheet`).
 */
const TurnRedoSheet: React.FC<{
  id: string;
  anchorRef: React.RefObject<HTMLElement | null>;
  children: React.ReactNode;
}> = ({ id, anchorRef, children }) => {
  const ref = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const place = () => {
      const anchor = anchorRef.current;
      if (!anchor || !ref.current) return;
      ref.current.style.setProperty("--turn-redo-pill-bottom", `${Math.ceil(anchor.getBoundingClientRect().bottom)}px`);
      // Never taller than the room below the game header.
      const header = document.querySelector(".app-shell__header");
      ref.current.style.setProperty("--turn-redo-top-limit", `${Math.ceil(header?.getBoundingClientRect().bottom ?? 0)}px`);
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [anchorRef]);
  return (
    <section ref={ref} id={id} className="turn-redo__sheet" data-testid="turn-redo-sheet" aria-label="Turn redo">
      {children}
    </section>
  );
};

/**
 * The strip for a turn redo in flight: waiting for the new turn, replaying the round, and where
 * the replay stopped. Restore and keep are for the host and the redoing seat; everyone else
 * only sees what is happening.
 */
export const TurnRedoBar: React.FC<TurnRedoBarProps> = ({
  status,
  busy,
  error,
  onAutoplay,
  onRestore,
  onKeep,
}) => {
  const display = usePlayerIdentity();
  const name = (seat: string) => display(seat).label;
  const compact = useCompactLayout();
  // A short landscape screen keeps the desktop grid: the strip of the board would sit under the bars,
  // so the pill docks at the bottom of the side column instead.
  const docked = useWideLayout();
  const prepSlot = useBoardPrepSlot()?.slot ?? null;
  const sheetId = useId();
  const pillRef = useRef<HTMLDivElement>(null);
  // The phone panel is collapsed unless the player has to decide; their own toggle wins for as long as the state stays.
  const [toggled, setToggled] = useState<{ state: string; open: boolean } | null>(null);
  const [moreDetail, setMoreDetail] = useState(false);
  // The bar the viewer sent away (viewer local); a different state of the redo shows it again.
  const [hiddenKey, setHiddenKey] = useState<string | null>(null);
  if (!status && !busy && !error) return null;

  let state = "idle";
  let title = "";
  let pill = "";
  let detail: React.ReactNode = null;
  let actions: "none" | "restore" | "decide" | "replay" = "none";
  const outcome = status?.outcome ?? null;

  if (!status) {
    state = busy === "restore" ? "restoring" : busy ? "rewinding" : "error";
    title =
      busy === "restore"
        ? "Restoring the original timeline…"
        : busy
          ? "Going back to the start of the turn…"
          : "The turn redo did not happen";
    pill = busy === "restore" ? "Restoring the original timeline…" : busy ? "Going back to the start of the turn…" : title;
  } else if (busy === "autoplay" || (status.stage === "new_turn" && status.turn_complete)) {
    state = "replaying";
    title = "Replaying the round…";
    pill = `Replaying… up to ${name(status.seat)}'s next turn`;
    detail = `${name(status.seat)}'s new turn is done. The recorded decisions that followed are being replayed, with the original dice and the cards they drew, up to ${name(status.seat)}'s next turn.`;
    // Restore stays while the replay request runs (disabled): taking it away and putting it back made
    // it flicker, and a click aimed at it could land on a detached element.
    actions = "replay";
  } else if (status.stage === "new_turn") {
    state = "new-turn";
    title = `Redoing ${name(status.seat)}'s ${status.turns_back === 2 ? "last two turns" : "last turn"}`;
    pill = title;
    detail =
      "Play the new turn. When it ends, the recorded decisions that followed replay on top of it, with the original dice and the cards they drew, and stop when your next turn begins.";
    actions = "restore";
  } else if (outcome?.stop.kind === "conflict") {
    const { conflict } = outcome.stop;
    state = "conflict";
    title = "The replay stopped";
    pill = title;
    detail = `${plural(outcome.kept, "recorded decision was", "recorded decisions were")} kept, then ${conflictReason(conflict, name)}. ${name(conflict.seat)} decides "${conflict.prompt}" now.`;
    actions = "decide";
  } else if (outcome?.stop.kind === "handoff") {
    state = "handoff";
    title = `Back to ${name(outcome.stop.seat)}: the next turn begins`;
    pill = title;
    detail = `${plural(outcome.kept, "recorded decision", "recorded decisions")} ${outcome.kept === 1 ? "was" : "were"} replayed and kept, including that seat's own reactions and votes. Nothing more is replayed; the rest of the original stays in the saved timeline.`;
    actions = "decide";
  } else if (outcome && outcome.tail_total === 0) {
    // The seat redid the turn it was still playing: nothing followed it, so nothing was replayed.
    state = "complete";
    title = `${name(status.seat)}'s new turn is done`;
    pill = title;
    detail =
      "Nothing had happened after the redone turn, so there was nothing to replay. Keep the new turn, or restore the original one.";
    actions = "decide";
  } else {
    state = "complete";
    title = "The recorded round was replayed";
    pill = title;
    detail = `All ${plural(outcome?.kept ?? 0, "recorded decision", "recorded decisions")} that followed the redone turn were kept.`;
    actions = "decide";
  }

  const control = status?.can_control ?? false;
  const barKey = `${state}:${status?.seat ?? ""}:${status?.rewound_to ?? ""}:${status?.redo_count ?? ""}`;
  if (hiddenKey === barKey) return null;
  // Restore / Keep are the answer while the viewer decides; any other state can be waved away.
  const canHide = !(control && (actions === "restore" || actions === "decide"));
  const hideButton = canHide && (
    <button
      type="button"
      className="button button--secondary button--sm turn-redo__hide"
      data-testid="turn-redo-hide"
      aria-label="Hide the turn redo bar"
      onClick={() => setHiddenKey(barKey)}
    >
      Hide
    </button>
  );

  const actionButtons = control && (
    <>
      {actions === "replay" && busy === null && (
        <button
          type="button"
          className="button button--secondary button--sm"
          data-testid="turn-redo-replay"
          onClick={onAutoplay}
        >
          Replay the round now
        </button>
      )}
      {(actions === "restore" || actions === "decide" || actions === "replay") && (
        <button
          type="button"
          className="button button--secondary button--sm"
          data-testid="turn-redo-restore"
          disabled={busy !== null}
          onClick={onRestore}
        >
          Restore original timeline
        </button>
      )}
      {actions === "decide" && (
        <button
          type="button"
          className="button button--primary button--sm"
          data-testid="turn-redo-keep"
          disabled={busy !== null}
          onClick={onKeep}
        >
          {state === "conflict" ? "Continue from here" : "Keep this timeline"}
        </button>
      )}
    </>
  );

  if (compact) {
    const mustDecide = control && actions === "decide";
    const expanded = toggled && toggled.state === state ? toggled.open : mustDecide || !!error;
    const longDetail = typeof detail === "string" && detail.length > LONG_DETAIL_CHARS;
    const compactBar = (
      <div
        className={`turn-redo turn-redo--compact${docked ? " turn-redo--dock" : prepSlot ? "" : " turn-redo--floating"}`}
        aria-busy={busy !== null || state === "replaying"}
        data-testid="turn-redo-bar"
        data-state={state}
        data-expanded={expanded}
      >
        <div className="turn-redo__pill" ref={pillRef}>
          <span className="turn-redo__pill-label" role="status" aria-live="polite">
            {pill}
          </span>
          <button
            type="button"
            className="button button--secondary button--sm turn-redo__toggle"
            data-testid="turn-redo-toggle"
            aria-expanded={expanded}
            aria-controls={sheetId}
            aria-label={expanded ? "Close the turn redo panel" : "Open the turn redo panel"}
            onClick={() => setToggled({ state, open: !expanded })}
          >
            {expanded ? "Close" : mustDecide ? "Decide" : "Open"}
          </button>
          {hideButton}
        </div>
        {expanded && (
          <TurnRedoSheet id={sheetId} anchorRef={pillRef}>
            <div className="turn-redo__sheet-head">
              <strong className="turn-redo__title">{title}</strong>
              <button
                type="button"
                className="button button--secondary button--sm turn-redo__close"
                data-testid="turn-redo-close"
                aria-label="Close the turn redo panel"
                onClick={() => setToggled({ state, open: false })}
              >
                Close
              </button>
            </div>
            <div className="turn-redo__sheet-body">
              {detail && (
                <span
                  className={`turn-redo__detail${longDetail && !moreDetail ? " turn-redo__detail--clamped" : ""}`}
                  data-testid="turn-redo-detail"
                >
                  {detail}
                </span>
              )}
              {longDetail && (
                <button
                  type="button"
                  className="turn-redo__more"
                  data-testid="turn-redo-more"
                  aria-expanded={moreDetail}
                  onClick={() => setMoreDetail((value) => !value)}
                >
                  {moreDetail ? "Less" : "Details"}
                </button>
              )}
              {error && (
                <span className="turn-redo__error text-danger" data-testid="turn-redo-error">
                  {error}
                </span>
              )}
            </div>
            {control && <div className="turn-redo__actions">{actionButtons}</div>}
          </TurnRedoSheet>
        )}
      </div>
    );
    return prepSlot && !docked ? createPortal(compactBar, prepSlot) : compactBar;
  }

  return (
    <div
      className="turn-redo"
      role="status"
      aria-live="polite"
      aria-busy={busy !== null || state === "replaying"}
      data-testid="turn-redo-bar"
      data-state={state}
    >
      <div className="turn-redo__text">
        <strong className="turn-redo__title">{title}</strong>
        {detail && <span className="turn-redo__detail">{detail}</span>}
        {error && (
          <span className="turn-redo__error text-danger" data-testid="turn-redo-error">
            {error}
          </span>
        )}
      </div>
      {(control || canHide) && (
        <div className="turn-redo__actions">
          {control && actionButtons}
          {hideButton}
        </div>
      )}
    </div>
  );
};
