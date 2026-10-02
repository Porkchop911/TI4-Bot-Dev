import React, { useState, useEffect, useMemo } from "react";
import { PendingChoiceDto } from "../protocol/types.ts";
import { usePipelineRunner, SemanticIntent } from "../hooks/usePipelineRunner.ts";
import { Dialog } from "../primitives/index.ts";
import { getAgendaPlanetVotes, ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";

export interface AgendaBallotModalProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  onSubmitBatch?: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
  selectedOptionId?: string;
}

export const AgendaBallotModal: React.FC<AgendaBallotModalProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  onSubmitBatch,
  isOpen,
  onClose,
  lastError,
  selectedOptionId,
}) => {
  const display = usePlayerIdentity();
  const subtype =
    choice?.context?.subtype === "vote_tiebreak"
      ? "vote_tiebreak"
      : model?.workflow
        ? model.workflow === "agenda_vote_planets"
          ? "vote_exhaust_planet"
          : model.workflow === "agenda_vote_outcome"
            ? "cast_vote"
            : (choice?.context?.subtype ?? "")
        : (choice?.context?.subtype ?? "");

  const isCastVote = subtype === "cast_vote";
  const isExhaustPlanet = subtype === "vote_exhaust_planet";
  const isTiebreak = subtype === "vote_tiebreak";

  // For planet basket in vote_exhaust_planet
  const [stagedPlanets, setStagedPlanets] = useState<string[]>([]);
  const [batchError, setBatchError] = useState<string | null>(null);
  const [batchRunning, setBatchRunning] = useState(false);

  const { executePipeline, isRunning: isPipelineRunning } = usePipelineRunner(choice, onSubmit);

  // Reset staging on nonce change
  useEffect(() => {
    setStagedPlanets([]);
    setBatchError(null);
  }, [choice?.nonce]);

  useEffect(() => {
    if (
      isExhaustPlanet &&
      selectedOptionId &&
      choice?.options.some((option) => option.id === selectedOptionId)
    ) {
      setStagedPlanets((ids) =>
        ids.includes(selectedOptionId) ? ids : [...ids, selectedOptionId],
      );
    }
  }, [selectedOptionId, choice?.nonce, isExhaustPlanet]);

  // Extract vote tallies if in cast_vote
  const outcomeTallies = useMemo(() => {
    if (!choice || !isCastVote) return [];
    return choice.options
      .filter((o) => o.id !== "decline" && o.kind !== "decline")
      .map((opt) => ({
        id: opt.id,
        label: opt.label,
        currentVotes:
          typeof opt.payload?.current_votes === "number" ? opt.payload.current_votes : null,
      }));
  }, [choice, isCastVote]);

  // Extract planet options if in vote_exhaust_planet
  const planetOptions = useMemo(() => {
    if (!choice || !isExhaustPlanet) return [];
    return choice.options
      .filter((o) => o.id !== "decline" && o.kind !== "decline")
      .map((opt) => {
        const votes = getAgendaPlanetVotes(opt);
        const payloadPlanetName =
          typeof opt.payload?.planet_name === "string" ? opt.payload.planet_name : null;
        return {
          id: opt.id,
          label: opt.label,
          planetName: payloadPlanetName || opt.label || "Planet",
          votes,
        };
      });
  }, [choice, isExhaustPlanet]);

  const totalStagedVotes = useMemo(() => {
    return stagedPlanets.reduce((sum, planetId) => {
      const p = planetOptions.find((opt) => opt.id === planetId);
      return sum + (p?.votes ?? 0);
    }, 0);
  }, [stagedPlanets, planetOptions]);

  const togglePlanetStage = (planetId: string) => {
    setStagedPlanets((prev) =>
      prev.includes(planetId) ? prev.filter((p) => p !== planetId) : [...prev, planetId],
    );
  };

  const handleCommitPlanetVotes = async (isDirectSubmitting: boolean) => {
    if (stagedPlanets.length === 0 || isPipelineRunning || isDirectSubmitting || batchRunning)
      return;
    if (onSubmitBatch) {
      setBatchRunning(true);
      setBatchError(null);
      try {
        await onSubmitBatch({
          kind: "agenda_vote_planets",
          steps: [
            ...stagedPlanets.map((planet) => ({ kind: "vote_planet" as const, planet })),
            ...(stagedPlanets.length < planetOptions.length
              ? [{ kind: "done_voting" as const }]
              : []),
          ],
        });
      } catch (error) {
        setBatchError(error instanceof Error ? error.message : String(error));
      } finally {
        setBatchRunning(false);
      }
      return;
    }

    const intents: SemanticIntent[] = stagedPlanets.map((planetId) => ({
      predicate: (opt) => opt.id === planetId,
    }));

    // After exhausting staged planets, finish with decline (end of planet exhaustion)
    intents.push({
      predicate: (opt) => opt.id === "decline" || opt.kind === "decline",
    });

    executePipeline(intents);
  };

  if (!isOpen || !choice) return null;

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Content
        data-testid="agenda-ballot-modal"
        className="agenda-dialog choice-workflow-dialog"
      >
        <div className="panel choice-workflow-modal">
          <Dialog.Title as="h2" className="visually-hidden">
            {choice.prompt}
          </Dialog.Title>
          <DecisionHeader
            actor={choice.actor}
            title={
              isCastVote
                ? "Choose a voting outcome"
                : isExhaustPlanet
                  ? "Spend influence to vote"
                  : isTiebreak
                    ? "Break the tie"
                    : choice.prompt
            }
            instruction={choice.prompt}
            onMinimize={onClose}
            titleTestId="agenda-ballot-title"
            minimizeTestId="close-agenda-modal"
          />

          <WorkflowShell
            choice={choice}
            model={model}
            viewerSeat={viewerSeat}
            onSubmit={onSubmit}
            lastError={batchError ?? lastError}
            spectatorNotice={`Observing council voting in progress for ${display(choice.actor).label}...`}
            spectatorNoticeTestId="spectator-agenda-notice"
            errorTestId="agenda-error-banner"
          >
            {({ isActor, isDirectSubmitting, declineOption, submitDirect }) => (
              <>
                {/* Stage 1: Cast Vote Outcome Selection */}
                {isActor && isCastVote && (
                  <div className="workflow-inline">
                    <div className="workflow-inline">
                      Select an outcome{declineOption ? ", or abstain" : ""}:
                    </div>

                    {/* Live Outcome Tallies */}
                    <div className="workflow-inline">
                      {outcomeTallies.map((outcome) => (
                        <button
                          key={outcome.id}
                          type="button"
                          data-testid={`vote-outcome-opt-${outcome.id}`}
                          onClick={() => submitDirect(outcome.id)}
                          disabled={isDirectSubmitting}
                          className="button button--secondary"
                        >
                          <span className="workflow-inline">{outcome.label}</span>
                          <span
                            data-testid={`outcome-tally-${outcome.id}`}
                            className="workflow-inline"
                          >
                            {outcome.currentVotes === null
                              ? ""
                              : `${outcome.currentVotes} votes cast`}
                          </span>
                        </button>
                      ))}
                    </div>

                    {declineOption && (
                      <div className="workflow-inline">
                        <button
                          type="button"
                          data-testid="abstain-vote-btn"
                          onClick={() => submitDirect(declineOption.id)}
                          disabled={isDirectSubmitting}
                          className="button button--secondary"
                        >
                          {declineOption.label || "Abstain from Voting"}
                        </button>
                      </div>
                    )}
                  </div>
                )}

                {/* Stage 2: Planet Exhaustion Influence Basket */}
                {isActor && isExhaustPlanet && (
                  <div className="workflow-inline">
                    <div className="workflow-inline">
                      Select an offered planet to exhaust for influence votes:
                    </div>

                    {/* Basket Tally Header */}
                    <div className="workflow-inline">
                      <span className="workflow-inline">Staged Votes:</span>
                      <span data-testid="staged-votes-counter" className="workflow-inline">
                        +{totalStagedVotes} Votes
                      </span>
                    </div>

                    {/* Planet Grid */}
                    <div className="workflow-inline">
                      {planetOptions.map((planet) => {
                        const isStaged = stagedPlanets.includes(planet.id);
                        return (
                          <button
                            key={planet.id}
                            type="button"
                            data-testid={`planet-card-${planet.id}`}
                            onClick={() => togglePlanetStage(planet.id)}
                            disabled={isPipelineRunning || isDirectSubmitting}
                            className="button button--secondary agenda-dialog__planet"
                            data-staged={isStaged}
                          >
                            <span className="workflow-inline">{planet.planetName}</span>
                            <span className="workflow-inline">
                              {planet.votes === null ? "Votes unknown" : `${planet.votes} v`}
                            </span>
                          </button>
                        );
                      })}
                    </div>

                    {stagedPlanets.length > 1 && (
                      <p className="text-muted">
                        Votes are submitted one decision at a time. Later planets and finishing must
                        be offered again; the sequence stops if interrupted.
                      </p>
                    )}
                    {/* Commit & Done Actions */}
                    <div className="workflow-inline">
                      {stagedPlanets.length > 0 && (
                        <button
                          type="button"
                          className="button button--secondary"
                          onClick={() => setStagedPlanets([])}
                        >
                          Reset selection
                        </button>
                      )}
                      {declineOption ? (
                        <button
                          type="button"
                          data-testid="done-voting-planets-btn"
                          onClick={() => submitDirect(declineOption.id)}
                          disabled={isPipelineRunning || isDirectSubmitting}
                          className="button button--secondary"
                        >
                          Done Voting
                        </button>
                      ) : (
                        <div />
                      )}

                      <button
                        type="button"
                        data-testid="commit-planet-votes-btn"
                        onClick={() => handleCommitPlanetVotes(isDirectSubmitting)}
                        disabled={
                          stagedPlanets.length === 0 || isPipelineRunning || isDirectSubmitting
                        }
                        className="button button--primary"
                      >
                        {isPipelineRunning
                          ? "Committing Votes..."
                          : `Cast votes${totalStagedVotes ? ` (+${totalStagedVotes} staged)` : ""}`}
                      </button>
                    </div>
                  </div>
                )}

                {/* Stage 3: Speaker Tiebreaker Gavel */}
                {isActor && isTiebreak && (
                  <div className="workflow-inline">
                    <div className="workflow-inline">
                      <span>⚖️</span>
                      <span>
                        The council vote is tied! As Speaker, you have the sole authority to break
                        the tie.
                      </span>
                    </div>

                    <div className="workflow-inline">
                      {choice.options.map((opt) => (
                        <button
                          key={opt.id}
                          type="button"
                          data-testid={`tiebreak-opt-${opt.id}`}
                          onClick={() => submitDirect(opt.id)}
                          disabled={isDirectSubmitting}
                          className="button button--primary"
                        >
                          Resolve in Favor of: {opt.label}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </WorkflowShell>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
