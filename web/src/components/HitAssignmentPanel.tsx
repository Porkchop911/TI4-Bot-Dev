import React, { useMemo, useState } from "react";
import type { PlacedUnitView } from "../protocol/types.ts";
import {
  addDestroy,
  addSustain,
  autoFill,
  canAddDestroy,
  canSustain,
  casualtyPlan,
  strandedCargo,
  type CasualtyStep,
  buildHitRows,
  hitsToCover,
  removeHit,
  stagedHits,
  type HitContext,
  type HitStaging,
} from "../presentation/hitAssignment.ts";
import { UnitIcon, getUnitDisplayName } from "./UnitIcon.tsx";

export interface HitAssignmentPanelProps {
  /** Stage hits here, then send them as one plan: nothing is submitted per click. */
  hits: number;
  units: PlacedUnitView[];
  sustainTypes: Set<string>;
  destroyable?: Set<string> | null;
  onlyFighters?: boolean;
  /** Fighters and ground forces in space against the fleet's transport capacity. */
  cargo?: { load: number; capacity: number } | null;
  title: string;
  disabled?: boolean;
  onSubmitPlan: (steps: CasualtyStep[]) => Promise<void>;
}

/**
 * Shared hit assignment for space combat, anti-fighter barrage and ground combat. Ships whose
 * choice matters (they can sustain damage or carry cargo) are a row each; the rest are grouped.
 */
export const HitAssignmentPanel: React.FC<HitAssignmentPanelProps> = ({
  hits,
  units,
  sustainTypes,
  destroyable,
  onlyFighters,
  cargo,
  title,
  disabled,
  onSubmitPlan,
}) => {
  const ctx: HitContext = useMemo(
    () => ({ units, sustainTypes, destroyable, onlyFighters }),
    [units, sustainTypes, destroyable, onlyFighters],
  );
  const rows = useMemo(() => buildHitRows(ctx), [ctx]);
  const [staging, setStaging] = useState<HitStaging>({});
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const needed = hitsToCover(hits, rows, ctx);
  const staged = stagedHits(staging);
  const locked = disabled || submitting;
  const stranded = cargo
    ? strandedCargo(rows, staging, cargo.load, cargo.capacity)
    : 0;

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onSubmitPlan(casualtyPlan(rows, staging));
    } catch (cause) {
      // The engine moved on or rejected the plan: start again from what it offers now.
      setError(cause instanceof Error ? cause.message : String(cause));
      setStaging({});
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="hit-assignment" data-testid="hit-assignment-panel">
      <div className="hit-assignment__header">
        <strong>{title}</strong>
        <span data-testid="hit-assignment-remaining">
          {Math.max(0, needed - staged)} of {needed} hit
          {needed === 1 ? "" : "s"} left to assign
        </span>
      </div>
      {cargo && cargo.load > 0 && (
        <p className="hit-assignment__cargo" data-testid="hit-assignment-cargo">
          Cargo in space: {cargo.load} (capacity {cargo.capacity})
          {stranded > 0 &&
            ` · these losses leave ${stranded} without capacity, and they are removed`}
        </p>
      )}
      <div className="hit-assignment__rows">
        {rows.map((row) => {
          const entry = staging[row.key] ?? { destroy: 0, sustain: false };
          const name = `${getUnitDisplayName(row.unitType, row.count)}${row.damaged ? " (damaged)" : ""}`;
          return (
            <div
              key={row.key}
              className="hit-assignment__row"
              data-testid={`hit-row-${row.key}`}
              data-staged={entry.destroy + (entry.sustain ? 1 : 0)}
            >
              <UnitIcon type={row.unitType} size={20} />
              <span className="hit-assignment__name">
                {name}
                {row.individual ? "" : ` ×${row.count}`}
                {row.capacity > 0 && ` · capacity ${row.capacity}`}
              </span>
              <span className="hit-assignment__hint">
                {row.individual
                  ? row.canSustain
                    ? "can sustain damage · 2 hits to destroy"
                    : row.damaged
                      ? "damaged · 1 hit to destroy"
                      : ""
                  : `takes up to ${row.count} hit${row.count === 1 ? "" : "s"}`}
              </span>
              <span
                className="hit-assignment__staged"
                data-testid={`hit-staged-${row.key}`}
              >
                {entry.sustain && entry.destroy > 0
                  ? "destroyed"
                  : entry.sustain
                    ? "sustains"
                    : entry.destroy > 0
                      ? `−${entry.destroy}`
                      : ""}
              </span>
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid={`hit-remove-${row.key}`}
                aria-label={`Remove a hit from ${name}`}
                disabled={locked || (entry.destroy === 0 && !entry.sustain)}
                onClick={() => setStaging((s) => removeHit(s, row))}
              >
                −
              </button>
              {row.canSustain && (
                <button
                  type="button"
                  className="button button--secondary button--sm"
                  data-testid={`hit-sustain-${row.key}`}
                  aria-label={`Sustain damage on ${name}`}
                  disabled={locked || !canSustain(row, staging, needed)}
                  onClick={() => setStaging((s) => addSustain(s, row))}
                >
                  Sustain
                </button>
              )}
              {!row.canSustain && <span aria-hidden="true" />}
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid={`hit-destroy-${row.key}`}
                aria-label={`Destroy ${name}`}
                disabled={locked || !canAddDestroy(row, staging, needed, ctx)}
                onClick={() => setStaging((s) => addDestroy(s, row))}
              >
                +
              </button>
            </div>
          );
        })}
      </div>
      <div className="hit-assignment__actions">
        <button
          type="button"
          className="button button--secondary"
          data-testid="hit-auto-assign"
          disabled={locked || staged >= needed}
          onClick={() => setStaging((s) => autoFill(rows, s, needed, ctx))}
        >
          Auto-assign
        </button>
        <button
          type="button"
          className="button button--secondary"
          data-testid="hit-reset"
          disabled={locked || staged === 0}
          onClick={() => setStaging({})}
        >
          Reset
        </button>
        <button
          type="button"
          className="button button--primary"
          data-testid="hit-confirm"
          disabled={locked || needed === 0 || staged !== needed}
          onClick={() => void confirm()}
        >
          Confirm hits
        </button>
      </div>
      {error && (
        <div
          className="hit-assignment__error"
          role="alert"
          data-testid="hit-assignment-error"
        >
          {error}
        </div>
      )}
    </div>
  );
};
