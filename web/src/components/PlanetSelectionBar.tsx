import React, { useState } from "react";
import { ValueText } from "./PlanetValueIcons.tsx";
import { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import {
  ChoiceRendererModel,
  isDeclineOption,
  isExtraPlanetSelectionOption,
  optionPlanetId,
} from "../presentation/choiceModel.ts";
import {
  PlanetDetails,
  describePlanetDecision,
  describeStructureStep,
  formatPlanetStats,
  formatStructureStep,
  getPlanetDetails,
  groupPlanetCandidates,
  isSpecificOptionLabel,
  optionActionLabel,
} from "../presentation/planetSelection.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { StructureInfo } from "./SystemFactsView.tsx";
import { UnitAbilityOptionNote } from "./UnitAbilityParts.tsx";
import { describeUnitAbilityOption } from "../presentation/unitAbilityOptions.ts";
import { isTransitDiodesChoice } from "../presentation/transitDiodes.ts";
import { TransitDiodesPanel } from "./TransitDiodesPanel.tsx";
import "./SystemFacts.css";

export interface PlanetSelectionBarProps {
  choice: PendingChoiceDto;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  selectedOptionId?: string;
  /** Planet picked on the map when several options share it (e.g. PDS vs. space dock). */
  selectedPlanetId?: string | null;
  onSelectOption?: (optionId: string) => void;
  /** When omitted the bar keeps the picked planet itself. */
  onSelectPlanet?: (planetId: string | null) => void;
  onSubmit: (optionId: string) => Promise<void>;
  boardView?: BoardView;
  lastError?: string | null;
}

const capitalize = (text: string) => (text ? text[0].toUpperCase() + text.slice(1) : text);

/** Renders `label` with the planet's mention emphasised (and shown by its display name). */
function withPlanetEmphasis(label: string, planet: PlanetDetails): React.ReactNode {
  const lower = label.toLowerCase();
  for (const needle of [planet.name, planet.id]) {
    const at = lower.indexOf(needle.toLowerCase());
    if (needle && at >= 0) {
      return (
        <>
          {label.slice(0, at)}
          <strong>{planet.name}</strong>
          {label.slice(at + needle.length)}
        </>
      );
    }
  }
  return (
    <>
      {label} <strong>{planet.name}</strong>
    </>
  );
}

function PlanetFacts({ planet }: { planet: PlanetDetails }) {
  const stats = formatPlanetStats(planet);
  const facts = [
    ...planet.traits,
    ...planet.techSpecialties.map((t) => `${t} skip`),
    ...(planet.legendary ? ["legendary"] : []),
    ...(planet.exhausted ? ["exhausted"] : []),
  ];
  return (
    <span className="planet-selection-bar__facts" data-testid="planet-selection-facts">
      {stats && (
        <span className="planet-selection-bar__stats">
          (<ValueText text={stats} />)
        </span>
      )}
      {facts.length > 0 && <span className="text-muted"> · {facts.join(" · ")}</span>}
    </span>
  );
}

export const PlanetSelectionBar: React.FC<PlanetSelectionBarProps> = ({
  choice,
  viewerSeat,
  selectedOptionId,
  selectedPlanetId,
  onSelectOption,
  onSelectPlanet,
  onSubmit,
  boardView,
  lastError,
}) => {
  const present = useParticipantText();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);
  const [localPlanetId, setLocalPlanetId] = useState<string | null>(null);

  const description = describePlanetDecision(choice);
  const structureStep = describeStructureStep(choice);
  const sourceLabel = description.sourceLabel;
  const isActor = Boolean(viewerSeat && choice.actor === viewerSeat);

  if (!isActor) {
    return (
      <aside
        data-testid="planet-selection-bar"
        className="choice-banner panel system-activation-bar planet-selection-bar"
        aria-label="Planet selection status"
      >
        <div className="system-activation-bar__body">
          <span className="system-activation-bar__hint text-muted">
            Waiting for {present(choice.actor)} to choose a planet
            {sourceLabel ? ` (${sourceLabel})` : ""}...
          </span>
        </div>
      </aside>
    );
  }

  // Transit Diodes offers source x ground force x destination: its own stepwise panel.
  if (isTransitDiodesChoice(choice)) {
    return (
      <TransitDiodesPanel
        choice={choice}
        boardView={boardView}
        selectedOptionId={selectedOptionId}
        selectedPlanetId={selectedPlanetId}
        onSelectOption={onSelectOption}
        onSubmit={onSubmit}
        lastError={lastError}
      />
    );
  }

  const candidates = groupPlanetCandidates(choice, boardView);
  const extras = choice.options.filter(isExtraPlanetSelectionOption);
  const declineOption = choice.options.find(isDeclineOption) ?? null;

  const pickedPlanetId = onSelectPlanet ? (selectedPlanetId ?? null) : localPlanetId;
  const selectedOption =
    choice.options.find((o) => o.id === selectedOptionId && !isDeclineOption(o)) ?? null;
  const activePlanetId = selectedOption ? optionPlanetId(selectedOption) : pickedPlanetId;
  const activeCandidate = candidates.find((c) => c.planetId === activePlanetId) ?? null;
  const activeOption =
    selectedOption ?? (activeCandidate?.options.length === 1 ? activeCandidate.options[0] : null);
  const activePlanet = activePlanetId ? getPlanetDetails(activePlanetId, boardView) : null;

  const setPlanet = (planetId: string | null) => {
    if (onSelectPlanet) onSelectPlanet(planetId);
    else setLocalPlanetId(planetId);
  };

  const pickPlanet = (planetId: string) => {
    const candidate = candidates.find((c) => c.planetId === planetId);
    setPlanet(planetId);
    onSelectOption?.(candidate?.options.length === 1 ? candidate.options[0].id : "");
    setSubmissionError(null);
  };

  const pickOption = (option: ChoiceOptionDto) => {
    setPlanet(optionPlanetId(option));
    onSelectOption?.(option.id);
    setSubmissionError(null);
  };

  const submit = async (optionId: string) => {
    if (isSubmitting) return;
    setIsSubmitting(true);
    setSubmissionError(null);
    try {
      await onSubmit(optionId);
    } catch (err: unknown) {
      setSubmissionError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCancel = () => {
    setPlanet(null);
    onSelectOption?.("");
    setSubmissionError(null);
  };

  const actionFor = (option: ChoiceOptionDto): React.ReactNode => {
    const planetId = optionPlanetId(option);
    if (!planetId) return capitalize(optionActionLabel(option, sourceLabel));
    const planet = getPlanetDetails(planetId, boardView);
    const ability = describeUnitAbilityOption(choice, option, boardView);
    if (ability) {
      return (
        <>
          {ability.title} {option.id.startsWith("transit|") ? "to" : "on"} <strong>{planet.name}</strong>
        </>
      );
    }
    if (isSpecificOptionLabel(option, sourceLabel, planet))
      return withPlanetEmphasis(optionActionLabel(option, sourceLabel), planet);
    return (
      <>
        {description.actionVerb} <strong>{planet.name}</strong>
      </>
    );
  };

  const currentVotes = (option: ChoiceOptionDto) =>
    typeof option.payload?.current_votes === "number" ? option.payload.current_votes : null;

  const errorMessage = submissionError || lastError;

  return (
    <aside
      data-testid="planet-selection-bar"
      className="choice-banner panel system-activation-bar planet-selection-bar"
      aria-label="Planet selection"
    >
      <div className="system-activation-bar__body">
        <div className="system-activation-bar__prompt-row" data-testid="planet-selection-prompt">
          <span className="badge badge--primary" data-testid="planet-selection-source">
            {sourceLabel ?? "Choose a planet"}
          </span>
          {structureStep && (
            <span className="badge" data-testid="structure-step">
              {formatStructureStep(structureStep)}
            </span>
          )}
          <span className="system-activation-bar__prompt">
            {present(capitalize(description.actionPrompt))}
          </span>
          {!activeOption && (
            <span className="system-activation-bar__hint text-muted">
              (Click a highlighted planet)
            </span>
          )}
        </div>

        {activeOption ? (
          <div className="system-activation-bar__confirm-row">
            <span className="system-activation-bar__title" data-testid="planet-selection-action">
              {sourceLabel && <>{sourceLabel} — </>}
              {actionFor(activeOption)}{" "}
              {activePlanet && optionPlanetId(activeOption) && (
                <PlanetFacts planet={activePlanet} />
              )}
              {currentVotes(activeOption) !== null && (
                <span className="text-muted"> · {currentVotes(activeOption)} votes cast</span>
              )}
            </span>
            <div className="system-activation-bar__actions">
              <button
                type="button"
                className="button button--primary"
                data-testid="confirm-planet-btn"
                disabled={isSubmitting}
                onClick={() => void submit(activeOption.id)}
              >
                {isSubmitting ? "Submitting..." : "Confirm"}
              </button>
              <button
                type="button"
                className="button button--secondary"
                data-testid="cancel-planet-btn"
                disabled={isSubmitting}
                onClick={handleCancel}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : activeCandidate && activePlanet ? (
          <div className="system-activation-bar__confirm-row">
            <span className="system-activation-bar__title" data-testid="planet-selection-action">
              {sourceLabel && <>{sourceLabel} — </>}
              on <strong>{activePlanet.name}</strong> <PlanetFacts planet={activePlanet} />
            </span>
            <div className="system-activation-bar__actions">
              {activeCandidate.options.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  className="button button--secondary"
                  data-testid={`planet-option-${option.id}`}
                  onClick={() => pickOption(option)}
                >
                  {capitalize(optionActionLabel(option, sourceLabel))}
                </button>
              ))}
              <button
                type="button"
                className="button button--secondary"
                data-testid="cancel-planet-btn"
                onClick={handleCancel}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : null}

        {activeOption && <UnitAbilityOptionNote choice={choice} option={activeOption} board={boardView} />}

        {choice.context?.subtype === "place_structure" && activePlanetId && (
          <StructureInfo planetId={activePlanetId} board={boardView} />
        )}

        <div
          className="planet-selection-bar__chips"
          role="group"
          aria-label="Candidate planets"
          data-testid="planet-selection-candidates"
        >
          {candidates.map((candidate) => {
            const planet = getPlanetDetails(candidate.planetId, boardView);
            const stats = formatPlanetStats(planet);
            const votes = currentVotes(candidate.options[0]);
            return (
              <button
                key={candidate.planetId}
                type="button"
                className="planet-selection-bar__chip"
                data-testid={`planet-chip-${candidate.planetId}`}
                aria-pressed={candidate.planetId === activePlanetId}
                disabled={isSubmitting}
                onClick={() => pickPlanet(candidate.planetId)}
              >
                {planet.name}
                {stats && (
                  <span className="text-muted">
                    {" "}
                    <ValueText text={stats} />
                  </span>
                )}
                {votes !== null && <span className="text-muted"> · {votes} votes</span>}
              </button>
            );
          })}
          {extras.map((option) => (
            <button
              key={option.id}
              type="button"
              className="planet-selection-bar__chip planet-selection-bar__chip--extra"
              data-testid={`extra-option-${option.id}`}
              aria-pressed={option.id === activeOption?.id}
              disabled={isSubmitting}
              onClick={() => pickOption(option)}
            >
              {capitalize(optionActionLabel(option, sourceLabel))}
            </button>
          ))}
          {declineOption && (
            <button
              type="button"
              className="button button--secondary button--sm"
              data-testid="decline-planet-btn"
              disabled={isSubmitting}
              onClick={() => void submit(declineOption.id)}
            >
              {declineOption.label && declineOption.label.toLowerCase() !== "decline"
                ? capitalize(declineOption.label)
                : "Skip"}
            </button>
          )}
        </div>

        {errorMessage && (
          <div
            role="alert"
            className="system-activation-bar__error text-danger"
            data-testid="planet-selection-error"
          >
            {errorMessage}
          </div>
        )}
      </div>
    </aside>
  );
};
