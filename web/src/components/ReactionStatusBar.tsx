import React, { useState, useEffect, useMemo } from "react";
import { readableEventTypes } from "../presentation/participantText.ts";
import type {
  BoardView,
  GameEvent,
  PendingChoiceDto,
  PlayerView,
  ReactionModes,
  ReactionModeSetting,
} from "../protocol/types.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { firstSentence, useCardTextPrefs, type CardTextPrefs } from "../presentation/cardTextPrefs.ts";
import { describeReaction, type ReactionCard } from "../presentation/reactionModel.ts";
import { humanizeId } from "../protocol/contentCatalog.ts";
import { DecisionHeader } from "./DecisionHeader.tsx";
import "./ReactionStatusBar.css";

export interface ReactionStatusBarProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose?: () => void;
  lastError?: string | null;
  autoPassTimeoutSeconds?: number;
  /** The public log: the fallback when the decision carries no trigger. */
  events?: readonly GameEvent[];
  boardView?: BoardView;
  activePlayerId?: string | null;
  activeSystemId?: string | null;
  players?: Record<string, PlayerView>;
  /** Highlights a system on the map (the "Show on map" link of the system involved). */
  onShowSystem?: (systemId: string) => void;
  /** This seat's "never offer" cards by printed name (server state). */
  reactionModes?: ReactionModes;
  /** Sets one card's mode on the server; absent when the viewer cannot (a spectator). */
  onSetReactionMode?: (card: string, mode: ReactionModeSetting) => void;
}

const CardBlock: React.FC<{
  card: ReactionCard | null;
  note?: string | null;
  prefs: CardTextPrefs;
  showWindow?: boolean;
  testId?: string;
}> = ({ card, note, prefs, showWindow = false, testId }) => {
  const text = card?.text || note || "";
  if (!card && !text) return null;
  const name = card?.name ?? "";
  const brief = text ? firstSentence(text) : null;
  const collapsed = brief !== null && prefs.isCollapsed(name || text);
  return (
    <div className="reaction-card" data-testid={testId} data-collapsed={collapsed || undefined}>
      {card && <div className="reaction-card__name">{card.name}</div>}
      {showWindow && card?.window && (
        <div className="reaction-card__window" data-testid="reaction-card-window">
          Window: {card.window}
        </div>
      )}
      {text && (
        <p className="reaction-card__text" data-testid="reaction-card-text">
          {collapsed ? brief : text}
        </p>
      )}
      {brief !== null && (
        <button
          type="button"
          className="reaction-card__toggle"
          data-testid={`reaction-inspect-text-${name || "note"}`}
          aria-expanded={!collapsed}
          onClick={() => prefs.toggle(name || text)}
        >
          {collapsed ? "Show full text" : "Shrink"}
        </button>
      )}
    </div>
  );
};

/** Faction and colour, as everywhere else: the seat badge, the player and the faction. */
const ActorChip: React.FC<{ id: string; viewerSeat?: string | null; faction?: string }> = ({
  id,
  viewerSeat,
  faction,
}) => {
  const display = usePlayerIdentity();
  const who = display(id);
  return (
    <span className="reaction-actor" data-testid="reaction-actor">
      {who.position != null && <SeatBadge position={who.position} />}
      <span className="reaction-actor__name">{id === viewerSeat ? "You" : who.label}</span>
      {faction && <span className="reaction-actor__faction">{humanizeId(faction)}</span>}
    </span>
  );
};

export const ReactionStatusBar: React.FC<ReactionStatusBarProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isOpen,
  onClose,
  lastError,
  autoPassTimeoutSeconds,
  events,
  boardView,
  activePlayerId,
  activeSystemId,
  players,
  onShowSystem,
  reactionModes,
  onSetReactionMode,
}) => {
  const display = usePlayerIdentity();
  const prefs = useCardTextPrefs();
  const isActor = Boolean(choice && viewerSeat && choice.actor === viewerSeat);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [secondsRemaining, setSecondsRemaining] = useState<number | null>(
    autoPassTimeoutSeconds ?? null,
  );
  const [isPinned, setIsPinned] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const reaction = useMemo(() => {
    if (!choice) return null;
    return describeReaction({
      choice,
      events,
      activePlayerId,
      activeSystemId: activeSystemId ?? boardView?.active_system ?? null,
      viewerSeat,
      board: boardView,
      commodities: (seat) => players?.[seat]?.commodities ?? null,
      playerLabel: (id) => display(id).label,
      systemLabel: (id) => {
        const planets = Object.keys(boardView?.systems?.[id]?.planets ?? {});
        return planets.length
          ? `System ${id} (${planets.map(humanizeId).join(", ")})`
          : `System ${id}`;
      },
    });
  }, [choice, events, activePlayerId, activeSystemId, boardView, viewerSeat, display, players]);

  // What another player's action card was aimed at, beyond the system the sentence names.
  const cardTargets = useMemo(() => {
    const facts = reaction?.facts;
    if (!facts || (facts.eventType !== "ACTION_CARD_PLAYED" && facts.eventType !== "ACTION_CARD_DISCARDED"))
      return [];
    return [
      facts.planetId && `planet ${humanizeId(facts.planetId)}`,
      facts.subjectId && `player ${facts.subjectId === viewerSeat ? "you" : display(facts.subjectId).label}`,
    ].filter((t): t is string => Boolean(t));
  }, [reaction, viewerSeat, display]);

  // Reset state on nonce change
  useEffect(() => {
    setIsSubmitting(false);
    setSecondsRemaining(autoPassTimeoutSeconds ?? null);
    setSubmissionError(null);
  }, [choice?.nonce, autoPassTimeoutSeconds]);

  // Use model.declineOption when available (centralized extraction), fallback to local search
  const declineOption = useMemo(() => {
    return (
      model?.declineOption ??
      choice?.options.find((o) => o.id === "decline" || o.kind === "decline") ??
      null
    );
  }, [choice, model]);

  const reactionOptions = useMemo(() => {
    if (!choice) return [];
    return choice.options.filter((o) => o.id !== "decline" && o.kind !== "decline");
  }, [choice]);

  // The second step of a multi-card window: the player already said yes and now picks the card.
  const isCardStep = Boolean(choice?.context?.subtype?.startsWith("play_reaction_"));

  const handleAction = async (optionId: string) => {
    if (isSubmitting) return;
    setIsSubmitting(true);
    try {
      await onSubmit(optionId);
    } catch (error) {
      setSubmissionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsSubmitting(false);
    }
  };

  // Keyboard navigation (Spacebar -> Pass, Enter -> Play Reaction)
  useEffect(() => {
    if (!isOpen || !choice || !isActor || isSubmitting) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if focused on an input element
      const activeTag = document.activeElement?.tagName?.toLowerCase();
      if (activeTag === "input" || activeTag === "textarea" || activeTag === "select") {
        return;
      }

      if (e.key === " " || e.code === "Space") {
        if (declineOption) {
          e.preventDefault();
          handleAction(declineOption.id);
        }
      } else if (e.key === "Enter") {
        if (reactionOptions.length > 0) {
          e.preventDefault();
          handleAction(reactionOptions[0].id);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, choice, isActor, isSubmitting, declineOption, reactionOptions]);

  // Countdown timer for auto-pass (if configured and not pinned)
  useEffect(() => {
    if (!isOpen || !choice || !isActor || isPinned || secondsRemaining === null || isSubmitting) {
      return;
    }

    if (secondsRemaining <= 0) {
      if (declineOption) {
        // Clear the countdown immediately before calling handleAction so the
        // effect cannot re-fire if isSubmitting changes while the network
        // call is in flight.
        setSecondsRemaining(null);
        handleAction(declineOption.id);
      }
      return;
    }

    const timer = setTimeout(() => {
      setSecondsRemaining((prev) => (prev !== null ? prev - 1 : null));
    }, 1000);

    return () => clearTimeout(timer);
  }, [isOpen, choice, isActor, isPinned, secondsRemaining, isSubmitting, declineOption]);

  if (!isOpen || !choice || !reaction) return null;

  return (
    <div
      role="region"
      aria-label="Reaction Window"
      data-testid="reaction-status-bar"
      className="reaction-status-bar"
      title={readableEventTypes(choice.prompt)}
    >
      <DecisionHeader
        actor={choice.actor}
        title={reaction.title}
        progress={
          secondsRemaining === null || isPinned
            ? undefined
            : `${secondsRemaining} seconds remaining`
        }
        onMinimize={() => onClose?.()}
      />

      <section
        className="reaction-status-bar__happened"
        data-testid="reaction-trigger-context"
        data-trigger-source={reaction.source}
        aria-label="What happened"
      >
        <h3 className="reaction-status-bar__heading">What happened</h3>
        {reaction.facts.actorId && (
          <ActorChip
            id={reaction.facts.actorId}
            viewerSeat={viewerSeat}
            faction={players?.[reaction.facts.actorId]?.faction}
          />
        )}
        <p data-testid="reaction-bar-prompt" className="reaction-status-bar__sentence">
          {reaction.sentence}
        </p>
        {reaction.inResponse && (
          <p className="reaction-status-bar__note">
            This was played in response to another reaction.
          </p>
        )}
        {reaction.facts.systemId && (
          <div className="reaction-status-bar__system">
            <span
              className="reaction-status-bar__trigger-target"
              data-testid={`reaction-trigger-system-${reaction.facts.systemId}`}
            >
              System {reaction.facts.systemId}
            </span>
            {onShowSystem && (
              <button
                type="button"
                className="reaction-card__toggle"
                data-testid="reaction-inspect-show-on-map"
                onClick={() => onShowSystem(reaction.facts.systemId!)}
              >
                Show on map
              </button>
            )}
          </div>
        )}
        {cardTargets.length > 0 && (
          <p className="reaction-status-bar__targets" data-testid="reaction-trigger-targets">
            Target: {cardTargets.join(", ")}
          </p>
        )}
        {reaction.note && <p className="reaction-status-bar__note">{reaction.note}</p>}
        <CardBlock card={reaction.facts.card} prefs={prefs} testId="reaction-trigger-card" />
      </section>

      {!isActor ? (
        <div data-testid="spectator-reaction-notice" className="reaction-status-bar__spectator">
          Waiting for reactions...
        </div>
      ) : (
        <>
          <section className="reaction-status-bar__can-now" aria-label="You can now">
            <h3 className="reaction-status-bar__heading">
              You can now
              {secondsRemaining !== null && !isPinned && (
                <span className="reaction-status-bar__countdown">({secondsRemaining}s)</span>
              )}
            </h3>
            <p className="reaction-status-bar__sentence" data-testid="reaction-can-now">
              {reaction.canNowSentence}
            </p>
            <ul className="reaction-status-bar__rows">
              {reaction.reactions.map((row) => (
                <li key={row.optionId} className="reaction-status-bar__row">
                  {!row.card && <div className="reaction-card__name">{row.name}</div>}
                  <CardBlock card={row.card} note={row.note} prefs={prefs} showWindow />
                  {row.effect && (
                    <p className="reaction-status-bar__effect" data-testid="reaction-row-effect">
                      {row.effect}
                    </p>
                  )}
                  {row.card && onSetReactionMode && (
                    <label className="reaction-status-bar__never">
                      <input
                        type="checkbox"
                        data-testid={`reaction-inspect-never-${row.card.name}`}
                        checked={reactionModes?.[row.card.name] === "never"}
                        disabled={isSubmitting}
                        onChange={(e) => {
                          const name = row.card!.name;
                          onSetReactionMode(name, e.target.checked ? "never" : "always");
                          // Nothing else to play here: the window is answered for the player.
                          if (e.target.checked && reaction.reactions.length === 1 && declineOption) {
                            void handleAction(declineOption.id);
                          }
                        }}
                      />
                      Never offer {row.card.name} again this game
                    </label>
                  )}
                </li>
              ))}
            </ul>
          </section>
          <div className="reaction-status-bar__actions">
            {reaction.reactions.map((row) => (
              <button
                key={row.optionId}
                type="button"
                data-testid={`play-reaction-btn-${row.optionId}`}
                onClick={() => handleAction(row.optionId)}
                disabled={isSubmitting}
                className="button button--primary reaction-status-bar__play"
              >
                {row.buttonLabel}
              </button>
            ))}

            {declineOption && (
              <button
                type="button"
                data-testid="pass-reaction-btn"
                onClick={() => handleAction(declineOption.id)}
                disabled={isSubmitting}
                className="button button--secondary reaction-status-bar__pass"
              >
                {isCardStep ? "Back, do not play a card (Spacebar)" : "Pass (Spacebar)"}
              </button>
            )}

            <label className="reaction-status-bar__pin">
              <input
                type="checkbox"
                data-testid="reaction-inspect-compact"
                checked={prefs.compact}
                onChange={(e) => prefs.setCompact(e.target.checked)}
              />
              Compact card text
            </label>

            <label className="reaction-status-bar__pin">
              <input
                type="checkbox"
                data-testid="pin-reaction-toggle"
                checked={isPinned}
                onChange={(e) => setIsPinned(e.target.checked)}
              />
              Pin
            </label>
          </div>
        </>
      )}

      {(lastError || submissionError) && (
        <div data-testid="reaction-error-badge" role="alert" className="reaction-status-bar__error">
          {submissionError || lastError}
        </div>
      )}
    </div>
  );
};
