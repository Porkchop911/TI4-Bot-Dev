import React, { useEffect, useRef, useState } from "react";
import type { BoardView, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import {
  fetchGroundOdds,
  normalizeFaction,
  type GroundOddsRequest,
} from "../services/advisorService.ts";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { UnitIcon, getUnitDisplayName } from "./UnitIcon.tsx";
import { WorkflowShell } from "./WorkflowShell.tsx";

export type Landing = { planet: string; unit: string; damaged: boolean };
const same = (a: Landing, b: Landing) =>
  a.planet === b.planet && a.unit === b.unit && a.damaged === b.damaged;
const fromOption = (option: PendingChoiceDto["options"][number]): Landing | null =>
  typeof option.payload?.planet === "string" && typeof option.payload.unit === "string"
    ? {
        planet: option.payload.planet,
        unit: option.payload.unit,
        damaged:
          option.payload.damaged === true ||
          (option.payload.damaged === undefined && option.label.includes("(damaged)")),
      }
    : null;

/** Confirmation submits one fresh engine offer per copy and pauses on any interruption. */
export const InvasionLandingTray: React.FC<{
  choice: PendingChoiceDto;
  board?: BoardView;
  players?: Record<string, PlayerView>;
  viewerSeat?: string | null;
  selectedOptionId?: string;
  onSubmit: (id: string) => Promise<void>;
  onClose: () => void;
  lastError?: string | null;
  draft?: Landing[];
  onDraftChange?: (draft: Landing[]) => void;
  embedded?: boolean;
}> = ({
  choice,
  board,
  players,
  viewerSeat,
  onSubmit,
  onClose,
  lastError,
  draft: controlledDraft,
  onDraftChange,
  embedded = false,
}) => {
  const system = board?.invasion?.system_id ?? board?.active_system ?? "";
  const [localDraft, setLocalDraft] = useState<Landing[]>([]);
  const draft = controlledDraft ?? localDraft;
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const setDraft = (update: Landing[] | ((current: Landing[]) => Landing[])) => {
    const next = typeof update === "function" ? update(draftRef.current) : update;
    draftRef.current = next;
    if (onDraftChange) onDraftChange(next);
    else setLocalDraft(next);
  };
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [odds, setOdds] = useState<{
    key: string;
    value: number | "loading" | "unavailable" | "no-battle";
  } | null>(null);
  const submitting = useRef(false);
  const submittedNonce = useRef<string | null>(null);
  const onSubmitRef = useRef(onSubmit);
  onSubmitRef.current = onSubmit;
  const origin = useRef({ actor: choice.actor, system });
  const options = choice.options
    .map((option) => ({ option, landing: fromOption(option) }))
    .filter(
      (entry): entry is { option: PendingChoiceDto["options"][number]; landing: Landing } =>
        entry.landing !== null,
    );
  const planets = [...new Set(options.map(({ landing }) => landing.planet))];

  const units = board?.systems[system]?.units ?? [];
  const [planet, setPlanet] = useState<string | null>(planets[0] ?? null);
  useEffect(() => {
    if (!planet && planets[0]) setPlanet(planets[0]);
  }, [planets, planet]);

  const previewKey = JSON.stringify([
    board?.invasion,
    planet,
    units,
    draft,
    players && Object.values(players).map(({ id, faction }) => [id, faction]),
  ]);

  useEffect(() => {
    if (
      !planet ||
      !board?.invasion ||
      board.invasion.phase !== "landing" ||
      viewerSeat !== choice.actor
    ) {
      return;
    }
    const local = units.filter((unit) => unit.planet === planet);
    const context = board.invasion.odds_context?.[planet];
    if (!context) {
      setOdds({ key: previewKey, value: "unavailable" });
      return;
    }
    const groundTypes = new Set(context.ground_force_types);
    const opponent = context.opponent;
    const mine = local.filter(
      (unit) => unit.owner === choice.actor && groundTypes.has(unit.unit_type),
    );
    const planned = draft.filter((item) => item.planet === planet);
    if (mine.length + planned.length === 0) {
      setOdds(null);
      return;
    }
    if (!opponent) {
      setOdds({ key: previewKey, value: "no-battle" });
      return;
    }
    if (!context.available) {
      setOdds({ key: previewKey, value: "unavailable" });
      return;
    }
    if (!players?.[choice.actor] || !players[opponent]) {
      setOdds({ key: previewKey, value: "unavailable" });
      return;
    }
    const count = (entries: { unit_type: string; damaged: boolean }[]) => {
      const unitsMap: Record<string, number> = {};
      const damagedMap: Record<string, number> = {};
      for (const entry of entries) {
        unitsMap[entry.unit_type] = (unitsMap[entry.unit_type] ?? 0) + 1;
        if (entry.damaged) damagedMap[entry.unit_type] = (damagedMap[entry.unit_type] ?? 0) + 1;
      }
      return { units: unitsMap, damaged: damagedMap };
    };
    const defenderForces = local.filter(
      (unit) => unit.owner === opponent && groundTypes.has(unit.unit_type),
    );
    const attack = count([
      ...mine,
      ...planned.map((item) => ({ unit_type: item.unit, damaged: item.damaged })),
    ]);
    const request: GroundOddsRequest = {
      attacker: { faction: normalizeFaction(players[choice.actor].faction), ...attack },
      defender: {
        faction: normalizeFaction(players[opponent].faction),
        ...count(defenderForces),
        guns: context.additional_guns,
      },
      harrow: context.harrow_units,
      simulations: 2000,
    };
    const controller = new AbortController();
    setOdds({ key: previewKey, value: "loading" });
    void fetchGroundOdds(request, controller.signal)
      .then((result) => {
        if (!controller.signal.aborted)
          setOdds({ key: previewKey, value: result.attacker_win_rate });
      })
      .catch(() => {
        if (!controller.signal.aborted) setOdds({ key: previewKey, value: "unavailable" });
      });
    return () => controller.abort();
  }, [previewKey, choice.actor, viewerSeat]);

  useEffect(() => {
    if (!running || submitting.current || submittedNonce.current === choice.nonce || !draft.length)
      return;
    if (
      choice.actor !== origin.current.actor ||
      system !== origin.current.system ||
      choice.context?.subtype !== "commit_ground_forces"
    ) {
      setRunning(false);
      setError(
        "Landing paused: another decision intervened. Accepted landings remain committed; review the remaining draft.",
      );
      return;
    }
    const next = draft[0];
    const offered = options.find(({ landing }) => same(landing, next));
    if (!offered) {
      setRunning(false);
      setError("Landing paused: the next force is no longer offered. Edit the remaining draft.");
      return;
    }
    submitting.current = true;
    const nonce = choice.nonce;
    void onSubmitRef
      .current(offered.option.id)
      .then(() => {
        submittedNonce.current = nonce;
        setDraft((current) => current.slice(1));
      })
      .catch((cause: unknown) => {
        setRunning(false);
        setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
        submitting.current = false;
      });
  }, [running, draft, choice.nonce, choice.actor, choice.context?.subtype, system]);

  useEffect(() => {
    if (running && draft.length === 0) setRunning(false);
  }, [running, draft.length]);

  const stock = (unit: string, damaged: boolean) =>
    board?.systems[system]?.units.filter(
      (piece) =>
        piece.owner === choice.actor &&
        !piece.planet &&
        piece.unit_type === unit &&
        piece.damaged === damaged,
    ).length ?? 1;

  const visibleOdds = odds?.key === previewKey ? odds.value : "loading";
  const available = (landing: Landing) =>
    stock(landing.unit, landing.damaged) -
    draft.filter((item) => item.unit === landing.unit && item.damaged === landing.damaged).length;

  const content = (
    <WorkflowShell
      choice={choice}
      viewerSeat={viewerSeat}
      onSubmit={onSubmit}
      lastError={lastError}
      errorTestId="invasion-error"
    >
      {({ isActor, isDirectSubmitting, submitDirect }) =>
        isActor && (
          <div className="invasion-landing-body">
            <p className="invasion-landing-instruction">
              Stage forces to planets with + and −. Only confirmed landings are public.
            </p>

            <div className="invasion-landing-planets-container">
              {planets.map((name) => {
                const planetUnitsAlready =
                  board?.systems[system]?.units.filter(
                    (piece) => piece.owner === choice.actor && piece.planet === name,
                  ).length ?? 0;
                const planetDraftCount = draft.filter((item) => item.planet === name).length;
                const isCurrentSelected = planet === name;
                const planetOptions = options.filter(({ landing }) => landing.planet === name);

                return (
                  <div
                    key={name}
                    className={`invasion-planet-landing-card ${isCurrentSelected ? "invasion-planet-landing-card--active" : ""}`}
                  >
                    <div className="invasion-planet-landing-card__header">
                      <div className="invasion-planet-landing-card__title-row">
                        <button
                          type="button"
                          className={`button ${isCurrentSelected ? "button--primary" : "button--secondary"} invasion-planet-select-btn`}
                          aria-label={name}
                          onClick={() => setPlanet(name)}
                        >
                          <span aria-hidden="true">🪐 </span>
                          {name}
                        </button>
                        <span className="invasion-planet-landing-card__meta">
                          Already on planet: {planetUnitsAlready} · Staged: {planetDraftCount}
                        </span>
                      </div>

                      {isCurrentSelected && board?.invasion?.phase === "landing" && (
                        <div
                          data-testid="invasion-odds"
                          className="invasion-planet-landing-card__odds"
                        >
                          <span className="invasion-odds-badge">
                            {visibleOdds === "no-battle"
                              ? "No ground battle expected"
                              : visibleOdds === "loading"
                                ? "Calculating…"
                                : visibleOdds === "unavailable"
                                  ? "Odds unavailable"
                                  : `Projected odds if these forces land against ${board.invasion.odds_context?.[name]?.opponent ?? "defender"}: ${Math.round(visibleOdds * 100)}%`}
                          </span>
                          {typeof visibleOdds === "number" && (
                            <p
                              className="text-muted"
                              style={{ margin: "4px 0 0", fontSize: "11px" }}
                            >
                              Excludes cards, Parley, optional deploy and unmodeled modifiers.
                            </p>
                          )}
                        </div>
                      )}
                      {!isCurrentSelected && planets.length > 1 && (
                        <button
                          type="button"
                          className="button button--secondary button--sm"
                          style={{ alignSelf: "flex-start", fontSize: "11px", padding: "2px 8px" }}
                          onClick={() => setPlanet(name)}
                        >
                          Select {name} for projected odds
                        </button>
                      )}
                    </div>

                    <div className="decision-frame__options invasion-landing-units-list">
                      {planetOptions.map(({ option, landing }) => {
                        const count = draft.filter(
                          (item) =>
                            item.planet === name &&
                            item.unit === landing.unit &&
                            item.damaged === landing.damaged,
                        ).length;
                        const avail = available(landing);
                        return (
                          <div className="workflow-card invasion-landing-row" key={option.id}>
                            <div className="invasion-landing-row__unit-info">
                              <UnitIcon type={landing.unit} />
                              <div>
                                <strong>
                                  {getUnitDisplayName(landing.unit)}
                                  {landing.damaged ? " (damaged)" : ""}
                                </strong>
                                <div className="text-muted invasion-landing-row__avail">
                                  {Math.max(0, avail)} in space
                                </div>
                              </div>
                            </div>
                            <div className="workflow-row invasion-landing-row__stepper">
                              <button
                                type="button"
                                className="button button--secondary button--icon invasion-stepper-btn"
                                aria-label={`Remove ${landing.unit} from ${name}`}
                                disabled={count === 0 || running}
                                onClick={() => {
                                  setError(null);
                                  setPlanet(name);
                                  setDraft((current) => {
                                    for (let i = current.length - 1; i >= 0; i--) {
                                      if (same(current[i]!, landing)) {
                                        return [...current.slice(0, i), ...current.slice(i + 1)];
                                      }
                                    }
                                    return current;
                                  });
                                }}
                              >
                                −
                              </button>
                              <span className="workflow-count">{count}</span>
                              <button
                                type="button"
                                className="button button--secondary button--icon invasion-stepper-btn"
                                aria-label={`${option.label} · ${Math.max(0, avail)} in space`}
                                disabled={running || avail <= 0}
                                onClick={() => {
                                  setError(null);
                                  setPlanet(name);
                                  setDraft((current) => [...current, landing]);
                                }}
                              >
                                +
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>

            {draft.length > 0 && (
              <p className="invasion-draft-summary">
                Remaining draft:{" "}
                {draft
                  .map((item) => `${item.unit}${item.damaged ? " (damaged)" : ""} → ${item.planet}`)
                  .join(", ")}
              </p>
            )}

            {error && (
              <p role="alert" className="workflow-error">
                {error}
              </p>
            )}

            <div className="workflow-actions">
              <button
                type="button"
                className="button button--secondary"
                disabled={running}
                onClick={() => {
                  setDraft([]);
                  setError(null);
                }}
              >
                Reset draft
              </button>
              <button
                type="button"
                className="button button--primary"
                disabled={!draft.length || running}
                onClick={() => {
                  origin.current = { actor: choice.actor, system };
                  submittedNonce.current = null;
                  setError(null);
                  setRunning(true);
                }}
              >
                Confirm landings
              </button>
              {choice.options.some((option) => option.id === "done_committing") && (
                <button
                  type="button"
                  className="button button--secondary"
                  disabled={running || isDirectSubmitting || draft.length > 0}
                  onClick={() => void submitDirect("done_committing")}
                >
                  Done committing
                </button>
              )}
            </div>
            {running && <p className="text-muted">Submitting landings one at a time…</p>}
          </div>
        )
      }
    </WorkflowShell>
  );

  if (embedded) {
    return (
      <div
        className="invasion-landing-tray-embedded"
        data-testid="invasion-landing-tray"
        aria-label="Ground force landing"
      >
        {content}
      </div>
    );
  }

  return (
    <section
      className="panel decision-frame"
      data-testid="invasion-landing-tray"
      aria-label="Ground force landing"
    >
      <DecisionHeader
        actor={choice.actor}
        title="Plan landings"
        instruction={choice.prompt}
        progress={system ? `Active system: ${system}` : undefined}
        onMinimize={onClose}
      />
      {content}
    </section>
  );
};
