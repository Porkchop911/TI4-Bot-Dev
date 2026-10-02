import React, { useMemo, useState, useEffect, useCallback } from "react";
import type { PlayerView, ObjectiveProgressView, PendingChoiceDto } from "../protocol/types.ts";
import type { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { Dialog } from "../primitives/index.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { getPublicObjectiveMeta, humanizeId } from "../protocol/contentCatalog.ts";
import "./ObjectivesModal.css";

export interface ObjectivesModalProps {
  isOpen: boolean;
  onClose: () => void;
  revealedObjectives?: readonly string[];
  scoredObjectives?: Record<string, string[]>;
  objectiveProgress?: Record<string, Record<string, ObjectiveProgressView>>;
  players?: Record<string, PlayerView> | PlayerView[];
  viewerSeat?: string | null;
  onInspectCard?: (subject: { kind: "publicObjective"; id: string }) => void;
  // Decision / scoring mode props:
  choice?: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  onSubmit?: (optionId: string) => Promise<void>;
  lastError?: string | null;
  selectedOptionId?: string;
  onSelectOption?: (optionId: string) => void;
  isScoringMode?: boolean;
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
}) => {
  const display = usePlayerIdentity();

  const activeScoringMode = Boolean(
    isScoringMode ||
    (choice &&
      (choice.context?.subtype === "score_objective" ||
        choice.context?.subtype === "imperial_score_objective" ||
        model?.workflow === "objective_scoring")),
  );

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
      return Array.from(scoreableOptionMap.keys());
    }
    return [];
  }, [revealedObjectives, scoreableOptionMap]);

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
      const meta = getPublicObjectiveMeta(id);
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
    const meta = getPublicObjectiveMeta(objectiveId);
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
                  <div className="objectives-matrix__progress-label">
                    <span>{progress.have}</span> / <span>{progress.threshold}</span>
                  </div>
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
            </td>
          );
        })}
      </tr>
    );
  };

  if (!isOpen) return null;

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Content
        data-testid="objectives-modal"
        className="choice-workflow-dialog objectives-dialog"
      >
        <div className="panel choice-workflow-modal objectives-modal-panel">
          <header className="objectives-modal__header">
            <div className="objectives-modal__title-group">
              <Dialog.Title as="h2" className="objectives-modal__title">
                {activeScoringMode ? "Score Public Objective" : "Public Objectives Matrix"}
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

          <div className="objectives-modal__scroll-body">
            {effectiveRevealedObjectives.length === 0 ? (
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
                        {getPublicObjectiveMeta(selectedOptionId).name}
                      </strong>{" "}
                      (+{getPublicObjectiveMeta(selectedOptionId).points} VP)
                    </>
                  ) : selectedOptionId === "decline" ? (
                    <>
                      Selected: <strong>Do not score (Decline)</strong>
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
