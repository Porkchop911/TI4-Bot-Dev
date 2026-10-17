import React, { useMemo, useState, useEffect, useCallback } from "react";
import type {
  BoardView,
  PlayerView,
  ObjectiveProgressView,
  PendingChoiceDto,
} from "../protocol/types.ts";
import type { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { describeImperialOutcome } from "../presentation/imperialOutcome.ts";
import { Dialog } from "../primitives/index.ts";
import { useMatchMedia } from "../hooks/useCompactLayout.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { humanizeId } from "../protocol/contentCatalog.ts";
import {
  readSecretScoringOption,
  scoringObjectiveMeta,
  secretPhaseLabel,
  type SecretScoringOption,
} from "../presentation/scoringOptions.ts";
import {
  BOUGHT_PROGRESS_HINT,
  boughtObjectiveUnit,
  boughtProgressText,
  isBoughtObjective,
} from "../presentation/boughtObjective.ts";
import { ValueUnit } from "./PlanetValueIcons.tsx";
import { SecretObjectiveProgress } from "./SecretObjectiveProgress.tsx";
import { NeighbourProgress } from "./NeighbourProgress.tsx";
import { secretProgressOf } from "../presentation/secretProgress.ts";
import { homeBlockSentence, lostHomePlanets, lostHomeText } from "../presentation/publicScoringBlock.ts";
import "./ObjectivesModal.css";

/**
 * `boughtProgressText` with the resource / influence word as the shared icon ("can pay 5 / 8
 * [icon] now"); trade goods and the unitless form stay words. The container's aria-label carries
 * the full sentence.
 */
const BoughtProgress: React.FC<{
  have: number;
  threshold: number;
  unit: ReturnType<typeof boughtObjectiveUnit>;
  scoringWindowOpen: boolean;
}> = ({ have, threshold, unit, scoringWindowOpen }) => {
  if (unit !== "resources" && unit !== "influence") {
    return <>{boughtProgressText(have, threshold, unit, scoringWindowOpen)}</>;
  }
  return (
    <>
      can pay {have} / {threshold} <ValueUnit kind={unit} /> now
      {scoringWindowOpen ? "" : " · pay at scoring"}
    </>
  );
};

export interface ObjectivesModalProps {
  isOpen: boolean;
  onClose: () => void;
  revealedObjectives?: readonly string[];
  scoredObjectives?: Record<string, string[]>;
  objectiveProgress?: Record<string, Record<string, ObjectiveProgressView>>;
  players?: Record<string, PlayerView> | PlayerView[];
  viewerSeat?: string | null;
  onInspectCard?: (subject: { kind: "publicObjective" | "secretObjective"; id: string }) => void;
  // Decision / scoring mode props:
  choice?: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  onSubmit?: (optionId: string) => Promise<void>;
  lastError?: string | null;
  selectedOptionId?: string;
  onSelectOption?: (optionId: string) => void;
  isScoringMode?: boolean;
  /** Names the systems a secret's progress counts (Learn the Secrets of the Cosmos). */
  board?: BoardView | null;
  /** Rule 61.16: seats barred from public objectives, with the home planets they do not control. */
  homeSystemLost?: Record<string, string[]>;
}

export const ObjectivesModal: React.FC<ObjectivesModalProps> = ({
  isOpen,
  onClose,
  revealedObjectives = [],
  scoredObjectives = {},
  objectiveProgress = {},
  players,
  viewerSeat,
  onInspectCard,
  choice,
  model,
  onSubmit,
  lastError,
  selectedOptionId: controlledSelectedOptionId,
  onSelectOption,
  isScoringMode = false,
  board,
  homeSystemLost,
}) => {
  const display = usePlayerIdentity();
  // On a phone the window is a bottom sheet over the map (like the Players drawer) so the dock
  // buttons stay usable; scoring keeps the modal. Same breakpoint as the drawers in index.css.
  const phoneWidth = useMatchMedia("(max-width: 720px)");
  const label = (seat: string) => display(seat).label;
  const homeLost = (seat: string) => lostHomePlanets({ home_system_lost: homeSystemLost }, board, seat);
  const viewerHomeLost = homeLost(viewerSeat ?? "");
  const imperial = choice ? describeImperialOutcome(choice) : null;
  const [imperialDetailsOpen, setImperialDetailsOpen] = useState(false);

  const activeScoringMode = Boolean(
    isScoringMode ||
    (choice &&
      (choice.context?.subtype === "score_objective" ||
        choice.context?.subtype === "imperial_score_objective" ||
        model?.workflow === "objective_scoring")),
  );

  const sheet = phoneWidth && !activeScoringMode;

  const scoreableOptionMap = useMemo(() => {
    if (!activeScoringMode || !choice?.options) return new Map<string, string>();
    const map = new Map<string, string>();
    for (const opt of choice.options) {
      if (opt.id !== "decline" && opt.kind !== "decline") {
        map.set(opt.id, opt.label);
      }
    }
    return map;
  }, [activeScoringMode, choice?.options]);

  const declineOption = useMemo(() => {
    if (!activeScoringMode || !choice?.options) return null;
    return choice.options.find((opt) => opt.id === "decline" || opt.kind === "decline") ?? null;
  }, [activeScoringMode, choice?.options]);

  const effectiveRevealedObjectives = useMemo(() => {
    if (revealedObjectives && revealedObjectives.length > 0) {
      return revealedObjectives;
    }
    if (scoreableOptionMap.size > 0) {
      // Secret options get their own section below, never a public row.
      return Array.from(scoreableOptionMap.keys()).filter((id) => !readSecretScoringOption(id));
    }
    return [];
  }, [revealedObjectives, scoreableOptionMap]);

  // Secrets this window may score (61.6 offers one public and one secret together), as cards.
  const secretOptions = useMemo(() => {
    const rows: { optionId: string; secret: SecretScoringOption }[] = [];
    for (const id of scoreableOptionMap.keys()) {
      if (revealedObjectives.includes(id)) continue; // a secret made public scores as public
      const secret = readSecretScoringOption(id);
      if (secret) rows.push({ optionId: id, secret });
    }
    return rows;
  }, [scoreableOptionMap, revealedObjectives]);

  const playerList = useMemo<PlayerView[]>(() => {
    if (players) {
      if (Array.isArray(players)) return players;
      return Object.values(players);
    }
    if (choice?.actor) {
      return [
        {
          id: choice.actor,
          faction: "sol",
          victory_points: 0,
          trade_goods: 0,
          commodities: 0,
          tactic_tokens: 0,
          fleet_tokens: 0,
          strategic_tokens: 0,
          passed: false,
          strategy_cards: [],
          exhausted_strategy_cards: [],
          technologies: [],
          exhausted_technologies: [],
          relics: [],
          exhausted_relics: [],
          action_cards_count: 0,
          secret_objectives_count: 0,
          held_action_cards: [],
          held_secret_objectives: [],
          scored_secret_objectives: [],
          leaders: {},
        },
      ];
    }
    return [];
  }, [players, choice?.actor]);

  // The asked seat's other secrets, shown beside the scoreable ones so the hand reads whole. Only
  // the seat's own view carries its hand, so this is empty for anybody else.
  // Outside a scoring window the viewer's own hand is listed too, with its progress.
  const heldSecretRows = useMemo(() => {
    if (!activeScoringMode) {
      if (!viewerSeat) return [];
      const own = playerList.find((player) => player.id === viewerSeat);
      return own?.held_secret_objectives ?? [];
    }
    if (!choice?.actor || (viewerSeat && viewerSeat !== choice.actor)) {
      return [];
    }
    const seat = playerList.find((player) => player.id === choice.actor);
    const offered = new Set(
      secretOptions.filter((row) => !row.secret.plotToken).map((row) => row.secret.secretId),
    );
    return (seat?.held_secret_objectives ?? []).filter((id) => !offered.has(id));
  }, [activeScoringMode, choice?.actor, viewerSeat, playerList, secretOptions]);

  const [uncontrolledSelectedOptionId, setUncontrolledSelectedOptionId] = useState<string | null>(
    null,
  );
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const selectedOptionId =
    controlledSelectedOptionId !== undefined
      ? controlledSelectedOptionId || null
      : uncontrolledSelectedOptionId;

  const setSelectedOptionId = useCallback(
    (id: string | null) => {
      if (controlledSelectedOptionId === undefined) {
        setUncontrolledSelectedOptionId(id);
      }
      if (id && onSelectOption) {
        onSelectOption(id);
      }
    },
    [controlledSelectedOptionId, onSelectOption],
  );

  useEffect(() => {
    setSubmissionError(null);
    if (!activeScoringMode || !choice?.options || choice.options.length === 0) {
      if (controlledSelectedOptionId === undefined) {
        setUncontrolledSelectedOptionId(null);
      }
      return;
    }

    const current =
      controlledSelectedOptionId !== undefined
        ? controlledSelectedOptionId
        : uncontrolledSelectedOptionId;

    // If current selection is already one of the valid offered options, do NOT reset it!
    if (current && choice.options.some((o) => o.id === current)) {
      return;
    }

    const firstScoreable = choice.options.find((o) => o.id !== "decline" && o.kind !== "decline");
    const initialId =
      firstScoreable?.id ??
      choice.options.find((o) => o.id === "decline" || o.kind === "decline")?.id ??
      null;

    if (initialId) {
      if (controlledSelectedOptionId === undefined) {
        setUncontrolledSelectedOptionId(initialId);
      }
      if (onSelectOption) {
        onSelectOption(initialId);
      }
    }
  }, [choice?.nonce, activeScoringMode]);

  const handleConfirm = async () => {
    if (!selectedOptionId || isSubmitting) return;
    setIsSubmitting(true);
    setSubmissionError(null);
    try {
      if (onSubmit) {
        await onSubmit(selectedOptionId);
      }
    } catch (err) {
      setSubmissionError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  const { stage1, stage2, other } = useMemo(() => {
    const s1: string[] = [];
    const s2: string[] = [];
    const rest: string[] = [];

    for (const id of effectiveRevealedObjectives) {
      const meta = scoringObjectiveMeta(id);
      if (meta.points === 1) {
        s1.push(id);
      } else if (meta.points === 2) {
        s2.push(id);
      } else {
        rest.push(id);
      }
    }

    return { stage1: s1, stage2: s2, other: rest };
  }, [effectiveRevealedObjectives]);

  const renderObjectiveRow = (objectiveId: string) => {
    const meta = scoringObjectiveMeta(objectiveId);
    const bought = isBoughtObjective(meta.description);
    const boughtUnit = boughtObjectiveUnit(meta.description);
    const isScoreable = scoreableOptionMap.has(objectiveId);
    const isSelected = selectedOptionId === objectiveId;

    return (
      <tr
        key={objectiveId}
        className={`objectives-matrix__row${isScoreable ? " objectives-matrix__row--scoreable" : ""}${
          isSelected ? " objectives-matrix__row--selected" : ""
        }`}
        data-testid={`objective-row-${objectiveId}`}
      >
        <td
          className={`objectives-matrix__card-cell${
            isScoreable ? " objectives-matrix__card-cell--scoreable" : ""
          }`}
          data-testid={isScoreable ? "choice-option" : undefined}
          data-option-id={isScoreable ? objectiveId : undefined}
          onClick={isScoreable ? () => setSelectedOptionId(objectiveId) : undefined}
          style={{ cursor: isScoreable ? "pointer" : "default" }}
        >
          <div className="objectives-matrix__card-content">
            <div className="objectives-matrix__card-title-row">
              {activeScoringMode && isScoreable && (
                <input
                  type="radio"
                  name="score-objective-choice"
                  value={objectiveId}
                  checked={isSelected}
                  onChange={() => setSelectedOptionId(objectiveId)}
                  className="objectives-matrix__radio"
                  aria-label={`Score ${meta.name}`}
                  style={{ cursor: "pointer" }}
                />
              )}
              <span
                className={`objectives-matrix__point-pill objectives-matrix__point-pill--stage-${meta.points}`}
              >
                {meta.points} VP
              </span>
              <button
                type="button"
                className="objectives-matrix__card-name-btn"
                data-testid={`inspect-objective-${objectiveId}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onInspectCard?.({ kind: "publicObjective", id: objectiveId });
                }}
              >
                {meta.name}
              </button>
              {isScoreable && isSelected && (
                <span className="objectives-matrix__selected-tag">Selected</span>
              )}
            </div>
            <div className="objectives-matrix__card-desc">{meta.description}</div>
            {viewerSeat && !scoredObjectives[viewerSeat]?.includes(objectiveId) && (
              <NeighbourProgress
                progress={objectiveProgress[viewerSeat]?.[objectiveId]}
                testId={`${objectiveId}-card`}
                className="neighbour-progress--card"
                heading="You:"
              />
            )}
          </div>
        </td>
        {playerList.map((player) => {
          const isScored = scoredObjectives[player.id]?.includes(objectiveId) ?? false;
          const progress = objectiveProgress[player.id]?.[objectiveId];
          const isSelf = viewerSeat === player.id;
          const isCurrentActor = choice?.actor === player.id;

          return (
            <td
              key={player.id}
              className={`objectives-matrix__progress-cell${
                isSelf ? " objectives-matrix__progress-cell--self" : ""
              }`}
              data-testid={`cell-${objectiveId}-${player.id}`}
              onClick={
                isCurrentActor && isScoreable ? () => setSelectedOptionId(objectiveId) : undefined
              }
              style={{ cursor: isCurrentActor && isScoreable ? "pointer" : "default" }}
            >
              {isScored ? (
                <div
                  className="objectives-matrix__status-scored"
                  data-testid={`status-scored-${objectiveId}-${player.id}`}
                >
                  <span className="objectives-matrix__icon-check">✓</span>
                  <span>Scored</span>
                </div>
              ) : isCurrentActor && isScoreable && isSelected ? (
                <div
                  className="objectives-matrix__status-selected"
                  data-testid={`status-selected-${objectiveId}-${player.id}`}
                >
                  <span className="objectives-matrix__icon-check">●</span>
                  <span>Selected</span>
                </div>
              ) : progress?.satisfied && !(isCurrentActor && isScoreable) && homeLost(player.id).length > 0 ? (
                <div
                  className="objectives-matrix__status-blocked"
                  data-testid={`status-blocked-${objectiveId}-${player.id}`}
                  title={`Met, but public objectives cannot be scored without ${lostHomeText(homeLost(player.id), label)} (rule 61.16)`}
                >
                  <span className="objectives-matrix__icon-blocked" aria-hidden="true">
                    !
                  </span>
                  <span>
                    Met ({progress.have}/{progress.threshold}) · home system lost
                  </span>
                </div>
              ) : progress?.satisfied || (isCurrentActor && isScoreable) ? (
                <div
                  className="objectives-matrix__status-ready"
                  data-testid={`status-ready-${objectiveId}-${player.id}`}
                >
                  <span className="objectives-matrix__icon-ready">✓</span>
                  <span className="objectives-matrix__ready-text">
                    {progress ? `Ready (${progress.have}/${progress.threshold})` : "Ready to Score"}
                  </span>
                </div>
              ) : progress ? (
                <div
                  className="objectives-matrix__status-progress"
                  data-testid={`status-progress-${objectiveId}-${player.id}`}
                >
                  {bought ? (
                    <div
                      className="objectives-matrix__progress-label objectives-matrix__progress-label--bought"
                      data-testid={`bought-progress-${objectiveId}-${player.id}`}
                      title={BOUGHT_PROGRESS_HINT}
                      aria-label={`${boughtProgressText(
                        progress.have,
                        progress.threshold,
                        boughtUnit,
                        activeScoringMode && isCurrentActor,
                      )}. ${BOUGHT_PROGRESS_HINT}`}
                    >
                      <BoughtProgress
                        have={progress.have}
                        threshold={progress.threshold}
                        unit={boughtUnit}
                        scoringWindowOpen={activeScoringMode && isCurrentActor}
                      />
                    </div>
                  ) : (
                    <div className="objectives-matrix__progress-label">
                      <span>{progress.have}</span> / <span>{progress.threshold}</span>
                    </div>
                  )}
                  <div className="objectives-matrix__progress-bar-bg">
                    <div
                      className="objectives-matrix__progress-bar-fill"
                      style={{
                        width: `${Math.min(100, Math.round((progress.have / progress.threshold) * 100))}%`,
                      }}
                    />
                  </div>
                </div>
              ) : (
                <span className="objectives-matrix__status-empty">—</span>
              )}
              {!isScored && (
                <NeighbourProgress progress={progress} testId={`${objectiveId}-${player.id}`} />
              )}
            </td>
          );
        })}
      </tr>
    );
  };

  const renderSecretRow = (
    key: string,
    secretId: string,
    optionId: string | null,
    plotToken: string | null,
  ) => {
    const meta = readSecretScoringOption(optionId ?? secretId)?.meta ?? scoringObjectiveMeta(secretId);
    const isScoreable = optionId !== null;
    const isSelected = isScoreable && selectedOptionId === optionId;
    // Only the viewer's own view carries progress, so reading it from the holder is safe.
    const holderId = activeScoringMode ? (choice?.actor ?? viewerSeat) : viewerSeat;
    const holder = playerList.find((player) => player.id === holderId);
    const select = isScoreable ? () => setSelectedOptionId(optionId) : undefined;
    return (
      <tr
        key={key}
        className={`objectives-matrix__row objectives-matrix__row--secret${
          isScoreable ? " objectives-matrix__row--scoreable" : " objectives-matrix__row--held"
        }${isSelected ? " objectives-matrix__row--selected" : ""}`}
        data-testid={`secret-row-${key}`}
      >
        <td
          className={`objectives-matrix__card-cell${
            isScoreable ? " objectives-matrix__card-cell--scoreable" : ""
          }`}
          data-testid={isScoreable ? "choice-option" : undefined}
          data-option-id={optionId ?? undefined}
          onClick={select}
          style={{ cursor: isScoreable ? "pointer" : "default" }}
        >
          <div className="objectives-matrix__card-content">
            <div className="objectives-matrix__card-title-row">
              {isScoreable && (
                <input
                  type="radio"
                  name="score-objective-choice"
                  value={optionId}
                  checked={isSelected}
                  onChange={select}
                  className="objectives-matrix__radio"
                  aria-label={`Score ${meta.name}`}
                  style={{ cursor: "pointer" }}
                />
              )}
              <span
                className={`objectives-matrix__point-pill objectives-matrix__point-pill--secret`}
              >
                {plotToken ? "Plot" : `${meta.points} VP`}
              </span>
              <button
                type="button"
                className="objectives-matrix__card-name-btn"
                data-testid={`inspect-secret-${key}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onInspectCard?.({ kind: "secretObjective", id: secretId });
                }}
              >
                {meta.name}
              </button>
              <span className="objectives-matrix__secret-phase" data-testid="secret-phase">
                {secretPhaseLabel(meta)}
              </span>
              {isSelected && <span className="objectives-matrix__selected-tag">Selected</span>}
            </div>
            <div className="objectives-matrix__card-desc" data-testid="secret-desc">
              {meta.description}
            </div>
            {/* In the card, not the status column: on a phone the matrix scrolls sideways and the
                status column starts off screen. */}
            {!isScoreable && (
              <SecretObjectiveProgress
                alias={secretId}
                progress={secretProgressOf(holder, secretId)}
                board={board}
              />
            )}
            {plotToken && (
              <div className="objectives-matrix__plot-note" data-testid="secret-plot-note">
                Plots Within Plots: scored by {display(plotToken).label}; it becomes a plot with
                their control token on it, for no VP.
              </div>
            )}
          </div>
        </td>
        <td
          colSpan={Math.max(playerList.length, 1)}
          className="objectives-matrix__progress-cell objectives-matrix__progress-cell--self objectives-matrix__secret-status"
          onClick={select}
          style={{ cursor: isScoreable ? "pointer" : "default" }}
        >
          {isSelected ? (
            <div className="objectives-matrix__status-selected">
              <span className="objectives-matrix__icon-check">●</span>
              <span>Selected</span>
            </div>
          ) : isScoreable ? (
            <div className="objectives-matrix__status-ready" data-testid={`secret-ready-${key}`}>
              <span className="objectives-matrix__icon-ready">✓</span>
              <span className="objectives-matrix__ready-text">
                {plotToken ? "Ready to score as a plot" : "Ready to Score"}
              </span>
            </div>
          ) : (
            <div className="objectives-matrix__secret-held" data-testid={`secret-held-${key}`}>
              <span className="objectives-matrix__status-empty">
                {activeScoringMode ? "In your hand · not scoreable now" : "In your hand"}
              </span>
            </div>
          )}
        </td>
      </tr>
    );
  };

  const hasSecretSection = secretOptions.length > 0 || heldSecretRows.length > 0;
  const title = !activeScoringMode
    ? "Public Objectives Matrix"
    : choice?.context?.subtype === "score_secret_objective"
      ? "Score Secret Objective"
      : secretOptions.length > 0
        ? "Score Objective"
        : "Score Public Objective";

  if (!isOpen) return null;

  return (
    <Dialog.Root
      open={isOpen}
      modal={!sheet}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Content
        id={sheet ? "objectives-drawer" : undefined}
        data-testid="objectives-modal"
        className={
          sheet
            ? "objectives-dialog objectives-dialog--sheet"
            : "choice-workflow-dialog objectives-dialog"
        }
        // A tap on the dimmed area outside the panel closes it, like the other panels.
        onClick={(e) => {
          if (!sheet && e.target === e.currentTarget) onClose();
        }}
      >
        <div className="panel choice-workflow-modal objectives-modal-panel">
          <header
            className={`objectives-modal__header${activeScoringMode ? " objectives-modal__header--scoring" : ""}`}
          >
            <div className="objectives-modal__title-group">
              <Dialog.Title as="h2" className="objectives-modal__title">
                {title}
                {activeScoringMode && (
                  <span className="objectives-modal__scoring-badge">Scoring Window</span>
                )}
              </Dialog.Title>
              <span className="objectives-modal__subtitle">
                {activeScoringMode
                  ? (choice?.prompt ??
                    "Select an unscored public objective to claim victory points.")
                  : `${effectiveRevealedObjectives.length} revealed · Track progress and scored objectives across all players`}
              </span>
            </div>
            {activeScoringMode ? (
              <button
                type="button"
                className="button button--secondary"
                style={{ padding: "6px 12px" }}
                data-testid="minimize-choice-button"
                aria-label="Minimize"
                onClick={onClose}
              >
                Minimize
              </button>
            ) : (
              <Dialog.Close
                className="button button--secondary"
                style={{ padding: "6px 12px" }}
                data-testid="objectives-modal-close"
                onClick={onClose}
              >
                Close
              </Dialog.Close>
            )}
          </header>

          {viewerHomeLost.length > 0 && (
            <div className="objectives-modal__home-lost" role="note" data-testid="home-system-lost">
              <span className="objectives-modal__outcome-tag">Home system lost</span>
              <span>{homeBlockSentence(viewerHomeLost, label)}</span>
            </div>
          )}

          {imperial && (
            <div
              className="objectives-modal__outcome"
              data-testid="imperial-outcome"
              data-variant={imperial.controlsMecatol ? "mecatol" : "secret"}
              data-open={imperialDetailsOpen ? "true" : "false"}
            >
              <span className="objectives-modal__outcome-tag">
                Imperial<span className="objectives-modal__outcome-tag-long"> · always applies</span>
              </span>
              <strong data-testid="imperial-outcome-headline">{imperial.headline}</strong>
              {/* Phones: the explanation folds away behind this button; desktop always shows it. */}
              <button
                type="button"
                className="objectives-modal__outcome-toggle"
                data-testid="imperial-outcome-toggle"
                aria-expanded={imperialDetailsOpen}
                aria-label={imperialDetailsOpen ? "Hide Imperial details" : "Show Imperial details"}
                onClick={() => setImperialDetailsOpen((open) => !open)}
              >
                {imperialDetailsOpen ? "Less" : "Why?"}
              </button>
              <span className="text-muted objectives-modal__outcome-details" data-testid="imperial-outcome-details">
                Scoring an objective is optional. This follows either way: holding Mecatol Rex
                gives the point, otherwise you draw the secret.
              </span>
            </div>
          )}

          <div className="objectives-modal__scroll-body">
            {effectiveRevealedObjectives.length === 0 && !hasSecretSection ? (
              <div className="objectives-modal__empty" data-testid="objectives-empty">
                No public objectives revealed yet.
              </div>
            ) : (
              <div className="objectives-matrix-container">
                <table className="objectives-matrix" data-testid="objectives-matrix-table">
                  <thead>
                    <tr>
                      <th className="objectives-matrix__card-col">Objective</th>
                      {playerList.map((player) => {
                        const identity = display(player.id);
                        const isSelf = viewerSeat === player.id;
                        return (
                          <th
                            key={player.id}
                            className={`objectives-matrix__player-col${
                              isSelf ? " objectives-matrix__player-col--self" : ""
                            }`}
                            data-testid={`player-col-header-${player.id}`}
                          >
                            <div className="objectives-matrix__player-header">
                              <span className="objectives-matrix__player-name">
                                {identity.label}
                                {isSelf && <span className="objectives-matrix__self-tag">You</span>}
                              </span>
                              <span className="objectives-matrix__player-faction">
                                {humanizeId(player.faction)} · {player.victory_points} VP
                              </span>
                            </div>
                          </th>
                        );
                      })}
                    </tr>
                  </thead>
                  <tbody>
                    {stage1.length > 0 && (
                      <tr className="objectives-matrix__stage-header-row">
                        <td colSpan={playerList.length + 1}>
                          <div className="objectives-matrix__stage-badge objectives-matrix__stage-badge--stage-1">
                            Stage I Objectives (1 VP)
                          </div>
                        </td>
                      </tr>
                    )}
                    {stage1.map((id) => renderObjectiveRow(id))}

                    {stage2.length > 0 && (
                      <tr className="objectives-matrix__stage-header-row">
                        <td colSpan={playerList.length + 1}>
                          <div className="objectives-matrix__stage-badge objectives-matrix__stage-badge--stage-2">
                            Stage II Objectives (2 VP)
                          </div>
                        </td>
                      </tr>
                    )}
                    {stage2.map((id) => renderObjectiveRow(id))}

                    {other.length > 0 && (
                      <tr className="objectives-matrix__stage-header-row">
                        <td colSpan={playerList.length + 1}>
                          <div className="objectives-matrix__stage-badge">Other Objectives</div>
                        </td>
                      </tr>
                    )}
                    {other.map((id) => renderObjectiveRow(id))}

                    {hasSecretSection && (
                      <tr className="objectives-matrix__stage-header-row">
                        <td colSpan={playerList.length + 1}>
                          <div
                            className="objectives-matrix__stage-badge objectives-matrix__stage-badge--secret"
                            data-testid="secret-objectives-header"
                          >
                            Your Secret Objectives
                          </div>
                        </td>
                      </tr>
                    )}
                    {secretOptions.map(({ optionId, secret }) =>
                      renderSecretRow(optionId, secret.secretId, optionId, secret.plotToken),
                    )}
                    {heldSecretRows.map((id) => renderSecretRow(`held-${id}`, id, null, null))}
                  </tbody>
                </table>
              </div>
            )}
          </div>

          {activeScoringMode && (
            <footer className="objectives-modal__footer" data-testid="objectives-scoring-footer">
              <div className="objectives-modal__footer-left">
                {(lastError || submissionError) && (
                  <div className="objectives-modal__error" role="alert">
                    {lastError || submissionError}
                  </div>
                )}
                <div className="objectives-modal__selection-summary">
                  {selectedOptionId && selectedOptionId !== "decline" ? (
                    <>
                      Selected:{" "}
                      <strong style={{ color: "#38bdf8" }}>
                        {scoringObjectiveMeta(selectedOptionId).name}
                      </strong>{" "}
                      {readSecretScoringOption(selectedOptionId)?.plotToken
                        ? "(as a plot, no VP)"
                        : `(+${scoringObjectiveMeta(selectedOptionId).points} VP)`}
                      {imperial && <> · {imperial.summary}</>}
                    </>
                  ) : selectedOptionId === "decline" ? (
                    <>
                      Selected: <strong>Do not score (Decline)</strong>
                      {imperial && <> · {imperial.summary}</>}
                    </>
                  ) : (
                    <span style={{ color: "#94a3b8" }}>Select an objective to score</span>
                  )}
                </div>
              </div>
              <div className="objectives-modal__footer-actions">
                {declineOption && (
                  <label
                    data-testid="choice-option"
                    data-option-id="decline"
                    className={`objectives-matrix__decline-label${
                      selectedOptionId === "decline"
                        ? " objectives-matrix__decline-label--selected"
                        : ""
                    }`}
                    onClick={() => setSelectedOptionId("decline")}
                  >
                    <input
                      type="radio"
                      name="score-objective-choice"
                      value="decline"
                      checked={selectedOptionId === "decline"}
                      onChange={() => setSelectedOptionId("decline")}
                      className="objectives-matrix__radio"
                      aria-label="Decline to score"
                    />
                    <span>Decline</span>
                  </label>
                )}
                <button
                  type="button"
                  data-testid="submit-choice-button"
                  className="button button--primary objectives-matrix__confirm-btn"
                  disabled={!selectedOptionId || isSubmitting}
                  onClick={handleConfirm}
                >
                  {isSubmitting
                    ? "Submitting..."
                    : selectedOptionId === "decline"
                      ? "Confirm decline"
                      : "Confirm score"}
                </button>
              </div>
            </footer>
          )}
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
