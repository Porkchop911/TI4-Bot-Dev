import React, { useState } from "react";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import type { BasketPlan } from "../protocol/client.ts";
import { PlanetValuePair, ValueUnit } from "./PlanetValueIcons.tsx";
import {
  useReadyPlanetsDraft,
  useSharedReadyDraft,
} from "../presentation/ReadyPlanetsDraftContext.tsx";
import { canConfirmReady, readyPlan, type ReadyPlanet } from "../presentation/readyPlanets.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { readReadyHints } from "../presentation/readyHints.ts";
import { ReadyHintsPanel } from "./ReadyHintsPanel.tsx";
import "./ReadyPlanetsBar.css";

export interface ReadyPlanetsBarProps {
  choice: PendingChoiceDto;
  viewerSeat?: string | null;
  boardView?: BoardView;
  onConfirm: (plan: BasketPlan) => Promise<void>;
  lastError?: string | null;
}

const count = (n: number) => (n === 1 ? "1 planet" : `${n} planets`);

function Chip({
  planet,
  picked,
  busy,
  onToggle,
}: {
  planet: ReadyPlanet;
  picked: boolean;
  busy: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className="ready-bar__chip"
      data-testid={`ready-planet-chip-${planet.id}`}
      aria-pressed={picked}
      disabled={busy}
      onClick={onToggle}
    >
      <span className="ready-bar__check" aria-hidden="true">
        {picked ? "✓" : ""}
      </span>
      <span className="ready-bar__name">{planet.name}</span>
      {planet.resources !== null && planet.influence !== null && (
        <PlanetValuePair resources={planet.resources} influence={planet.influence} planetId={planet.id} />
      )}
    </button>
  );
}

/**
 * Diplomacy's ready question as one choice: every exhausted planet is a toggle here and on the
 * map, and a single Confirm sends all picks as one batch. The same compact bar holds the whole
 * decision, so the map above it stays tappable; "Hide" folds it down to the summary and Confirm.
 */
export const ReadyPlanetsBar: React.FC<ReadyPlanetsBarProps> = ({
  choice,
  viewerSeat,
  boardView,
  onConfirm,
  lastError,
}) => {
  const present = useParticipantText();
  const shared = useSharedReadyDraft();
  const local = useReadyPlanetsDraft(shared ? null : choice, boardView);
  const { view, selected, toggle, reset } = shared ?? local;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  if (!view) return null;
  const hints = readReadyHints(choice.details);
  const isActor = Boolean(viewerSeat && choice.actor === viewerSeat);
  const ready = canConfirmReady(view, selected) && !busy && isActor;
  const left = view.required - selected.length;

  const confirm = async () => {
    const plan = readyPlan(view, selected);
    if (!plan || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm(plan);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      reset();
    } finally {
      setBusy(false);
    }
  };

  const shownError = error ?? lastError;
  return (
    <aside
      data-testid="ready-planets-bar"
      className="choice-banner panel system-activation-bar ready-bar"
      aria-label="Ready planets"
    >
      <div className="system-activation-bar__body ready-bar__body">
        <div className="ready-bar__summary" data-testid="ready-bar-summary">
          <span className="badge badge--primary">Diplomacy</span>
          <span data-testid="ready-bar-prompt">
            {view.required === 1 ? "Ready this planet" : `Ready ${count(view.required)}`}
          </span>
          <span
            className="ready-bar__count text-muted"
            data-testid="ready-bar-count"
            data-picked={selected.length}
          >
            {selected.length} of {view.required} picked
          </span>
          <button
            type="button"
            className="button button--secondary button--sm ready-bar__toggle"
            data-testid="ready-bar-collapse"
            aria-expanded={!collapsed}
            onClick={() => setCollapsed((value) => !value)}
          >
            {collapsed ? "Show ▴" : "Hide ▾"}
          </button>
        </div>
        {!collapsed && (
          <div
            className="ready-bar__chips"
            role="group"
            aria-label="Exhausted planets"
            data-testid="ready-bar-chips"
          >
            {view.planets.map((planet) => (
              <Chip
                key={planet.id}
                planet={planet}
                picked={selected.includes(planet.id)}
                busy={busy || !isActor}
                onToggle={() => toggle(planet.id)}
              />
            ))}
          </div>
        )}
        {!collapsed && (
          <div className="ready-bar__hint text-muted" data-testid="ready-bar-hint">
            {left > 0
              ? `Tap ${left === 1 ? "one more planet" : "planets"} here or on the map`
              : "Confirm to ready them (tap a planet again to change it)"}{" "}
            <ValueUnit kind="resources" /> <ValueUnit kind="influence" />
          </div>
        )}
        {!collapsed && hints && <ReadyHintsPanel hints={hints} showPlanets={false} />}
        <div className="system-activation-bar__actions ready-bar__actions">
          <button
            type="button"
            className="button button--primary"
            data-testid="ready-bar-confirm"
            aria-label={`Confirm: ready ${count(view.required)}`}
            disabled={!ready}
            onClick={() => void confirm()}
          >
            {busy ? "Submitting..." : "Confirm"}
          </button>
          {!collapsed && selected.length > 0 && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="ready-bar-clear"
              disabled={busy}
              onClick={reset}
            >
              Clear
            </button>
          )}
        </div>
        {shownError && (
          <div
            role="alert"
            className="system-activation-bar__error text-danger"
            data-testid="ready-bar-error"
          >
            {present(shownError)}
          </div>
        )}
      </div>
    </aside>
  );
};
