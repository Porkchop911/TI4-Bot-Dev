import React, { useEffect, useMemo, useRef, useState } from "react";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import { PlanetValuePair } from "./PlanetValueIcons.tsx";
import { UnitIcon } from "./UnitIcon.tsx";
import {
  buildTransitModel,
  describeTransitMove,
  describeTransitSource,
  findTransitForce,
  transitForceKey,
  type TransitSource,
} from "../presentation/transitDiodes.ts";
import { summarizeUnits } from "../presentation/systemFacts.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import "./TransitDiodesPanel.css";

export interface TransitDiodesPanelProps {
  choice: PendingChoiceDto;
  boardView?: BoardView;
  /** A move picked elsewhere (prepared answer); a transit option id preselects that move. */
  selectedOptionId?: string;
  /** Planet tapped on the map: picks the source (step 1) or the destination (step 2). */
  selectedPlanetId?: string | null;
  onSelectOption?: (optionId: string) => void;
  onSubmit: (optionId: string) => Promise<void>;
  lastError?: string | null;
}

function Values({ resources, influence }: { resources: number | null; influence: number | null }) {
  if (resources === null || influence === null) return null;
  return <PlanetValuePair resources={resources} influence={influence} className="transit-panel__values" />;
}

function sourceTroops(source: TransitSource, board: BoardView | undefined, actor: string): string | null {
  const units = (board?.systems?.[source.systemId]?.units ?? []).filter(
    (u) => u.owner === actor && (source.source === "space" ? !u.planet : u.planet === source.source),
  );
  return units.length ? summarizeUnits(units) : null;
}

/**
 * Transit Diodes in steps: pick a ground force (grouped by system and planet, with the planet's
 * values and your units there), then the planet it goes to (grouped by system, with what already
 * stands there), then Confirm. Every answer is one of the engine's own option ids; the list
 * scrolls inside the panel, so a dozen planets and a hundred moves stay reachable on a phone.
 */
export const TransitDiodesPanel: React.FC<TransitDiodesPanelProps> = ({
  choice,
  boardView,
  selectedOptionId,
  selectedPlanetId,
  onSelectOption,
  onSubmit,
  lastError,
}) => {
  const present = useParticipantText();
  const model = useMemo(() => buildTransitModel(choice, boardView), [choice, boardView]);
  const preset = selectedOptionId ? (model.moves.get(selectedOptionId) ?? null) : null;
  const [forceKey, setForceKey] = useState<string | null>(
    preset ? transitForceKey(preset.sourceSystem, preset.source, preset.unit) : null,
  );
  const [optionId, setOptionId] = useState<string | null>(preset?.optionId ?? null);
  const [collapsed, setCollapsed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // A new question (the next of up to four moves) starts over at step 1.
  useEffect(() => {
    setForceKey(null);
    setOptionId(null);
    setError(null);
  }, [choice.nonce]);

  // A prepared answer arriving after mount.
  useEffect(() => {
    const move = selectedOptionId ? model.moves.get(selectedOptionId) : undefined;
    if (!move) return;
    setForceKey(transitForceKey(move.sourceSystem, move.source, move.unit));
    setOptionId(move.optionId);
  }, [selectedOptionId, model]);

  const picked = findTransitForce(model, forceKey);
  const move = optionId ? (model.moves.get(optionId) ?? null) : null;

  const pickForce = (key: string | null) => {
    setForceKey(key);
    setOptionId(null);
    setError(null);
    onSelectOption?.("");
    listRef.current?.scrollTo?.({ top: 0 });
  };
  const pickDestination = (id: string) => {
    setOptionId(id);
    setError(null);
    onSelectOption?.(id);
  };

  // Map taps: a destination of the picked force, else a planet whose forces can leave.
  const lastMapPlanet = useRef<string | null | undefined>(selectedPlanetId);
  useEffect(() => {
    if (selectedPlanetId === lastMapPlanet.current) return;
    lastMapPlanet.current = selectedPlanetId;
    if (!selectedPlanetId) return;
    if (picked) {
      const destination = picked.force.destinations
        .flatMap((g) => g.items)
        .find((d) => d.planetId === selectedPlanetId);
      if (destination) {
        setOptionId(destination.optionId);
        return;
      }
    }
    const source = model.sources.flatMap((g) => g.items).find((s) => s.source === selectedPlanetId);
    if (source && source.forces.length === 1) pickForce(source.forces[0].key);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedPlanetId]);

  const submit = async (id: string) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(id);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const step = move ? 3 : picked ? 2 : 1;
  const shownError = error ?? lastError;
  const decline = model.declineOption;

  return (
    <aside
      data-testid="transit-panel"
      data-step={step}
      className="choice-banner panel system-activation-bar transit-panel"
      aria-label="Transit Diodes"
    >
      <div className="transit-panel__header">
        <span className="badge badge--primary">Transit Diodes</span>
        <span className="transit-panel__title">Redeploy ground forces</span>
        {model.movesLeft !== null && (
          <span className="transit-panel__left text-muted" data-testid="transit-moves-left">
            {model.movesLeft} {model.movesLeft === 1 ? "move" : "moves"} left
          </span>
        )}
        <button
          type="button"
          className="button button--secondary button--sm transit-panel__toggle"
          data-testid="transit-collapse"
          aria-expanded={!collapsed}
          onClick={() => setCollapsed((value) => !value)}
        >
          {collapsed ? "Show ▴" : "Hide ▾"}
        </button>
      </div>

      {!collapsed && (
        <ol className="transit-panel__steps" aria-label="Steps">
          <li aria-current={step === 1 ? "step" : undefined}>1 What moves</li>
          <li aria-current={step === 2 ? "step" : undefined}>2 Where to</li>
          <li aria-current={step === 3 ? "step" : undefined}>3 Confirm</li>
        </ol>
      )}

      {!collapsed && picked && (
        <div className="transit-panel__picked" data-testid="transit-picked">
          <UnitIcon type={picked.force.unit} size={20} />
          <span>
            Moving 1 <strong>{describeTransitSource(picked.source, picked.force, boardView)}</strong>
          </span>
          <button
            type="button"
            className="button button--secondary button--sm"
            data-testid="transit-change-source"
            disabled={busy}
            onClick={() => pickForce(null)}
          >
            Change
          </button>
        </div>
      )}

      {!collapsed && (
        <div className="transit-panel__list" ref={listRef} data-testid="transit-list">
          {!picked &&
            model.sources.map((group) => (
              <section key={group.systemId} className="transit-panel__group" data-testid={`transit-source-system-${group.systemId}`}>
                <h4 className="transit-panel__system">{group.tag}</h4>
                {group.items.map((source) => {
                  const troops = sourceTroops(source, boardView, choice.actor);
                  return (
                    <div key={source.key} className="transit-panel__source" data-testid={`transit-source-${source.key}`}>
                      <div className="transit-panel__place">
                        <span className="transit-panel__name">{source.name}</span>
                        <Values resources={source.resources} influence={source.influence} />
                        {troops && <span className="transit-panel__here text-muted">You have: {troops}</span>}
                      </div>
                      <div className="transit-panel__forces">
                        {source.forces.map((force) => (
                          <button
                            key={force.key}
                            type="button"
                            className="transit-panel__force"
                            data-testid={`transit-force-${force.key}`}
                            disabled={busy}
                            onClick={() => pickForce(force.key)}
                          >
                            <UnitIcon type={force.unit} size={20} />
                            <span>
                              {force.unitName}
                              {force.count > 1 ? ` ×${force.count}` : ""}
                            </span>
                          </button>
                        ))}
                      </div>
                    </div>
                  );
                })}
              </section>
            ))}
          {picked &&
            picked.force.destinations.map((group) => (
              <section key={group.systemId} className="transit-panel__group" data-testid={`transit-destination-system-${group.systemId}`}>
                <h4 className="transit-panel__system">{group.tag}</h4>
                {group.items.map((destination) => (
                  <button
                    key={destination.optionId}
                    type="button"
                    className="transit-panel__destination"
                    data-testid={`transit-destination-${destination.planetId}`}
                    data-option-id={destination.optionId}
                    aria-pressed={destination.optionId === optionId}
                    disabled={busy}
                    onClick={() => pickDestination(destination.optionId)}
                  >
                    <span className="transit-panel__name">{destination.name}</span>
                    <Values resources={destination.resources} influence={destination.influence} />
                    <span className="transit-panel__here text-muted">
                      {destination.here ? `Here now: ${destination.here}` : "No units here"}
                    </span>
                  </button>
                ))}
              </section>
            ))}
        </div>
      )}

      <div className="transit-panel__footer">
        <div className="transit-panel__summary" data-testid="transit-summary" aria-live="polite">
          {move
            ? describeTransitMove(move, boardView)
            : picked
              ? "Pick the planet it moves to (list or map)"
              : "Pick a ground force to move"}
        </div>
        <div className="transit-panel__actions">
          <button
            type="button"
            className="button button--primary"
            data-testid="transit-confirm"
            disabled={!move || busy}
            onClick={() => move && void submit(move.optionId)}
          >
            {busy ? "Submitting..." : "Confirm"}
          </button>
          {picked && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="transit-back"
              disabled={busy}
              onClick={() => pickForce(null)}
            >
              Back
            </button>
          )}
          {decline && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="transit-decline"
              disabled={busy}
              onClick={() => void submit(decline.id)}
            >
              {decline.label && decline.label.toLowerCase() !== "decline"
                ? decline.label[0].toUpperCase() + decline.label.slice(1)
                : "Skip"}
            </button>
          )}
        </div>
        {shownError && (
          <div role="alert" className="system-activation-bar__error text-danger" data-testid="transit-error">
            {present(shownError)}
          </div>
        )}
      </div>
    </aside>
  );
};
