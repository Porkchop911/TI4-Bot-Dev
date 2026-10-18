import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import React, { useEffect, useRef, useState } from "react";
import type { BoardView, GroundOddsRequest, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { groundOdds } from "@ti4/shared/engine/odds.ts";
import { useOddsEngine } from "./OddsEngine.tsx";
import { getPlanetEffectiveValues } from "../presentation/playerStats.ts";
import { wireBoosts } from "@ti4/shared/planets/values.ts";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { UnitIcon, getUnitDisplayName } from "./UnitIcon.tsx";
import { PlanetValue } from "./PlanetValueIcons.tsx";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { InfoPopover } from "./InfoPopover.tsx";
import {
  formatDefenderUnits,
  planDefaultLanding,
  planetDefense,
  planetInvasionInfo,
  planetStanding,
  sameDraft,
  type Landing,
  type PlanPlanet,
} from "../presentation/landingPlan.ts";

export type { Landing };
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
  /** The live decision is not this landing offer (the tray keeps the last one while hidden). */
  interrupted?: boolean;
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
  interrupted = false,
}) => {
  const present = useParticipantText();
  const oddsEngine = useOddsEngine();
  const oddsQuery = oddsEngine.state === "ready" ? oddsEngine.query : null;
  const oddsPending = oddsEngine.state === "loading";
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

  /** Resources/influence with attachment modifiers (map tile data, catalog fallback), plus trait. */
  const planetInfo = (planetId: string) => {
    const planetView = Object.values(board?.systems[system]?.planets ?? {}).find(
      (view) => view.planet_id === planetId,
    );
    const attachments = planetView?.attachments;
    const meta = board?.map_tiles
      ?.flatMap((tile) => tile.planets ?? [])
      .find((entry) => entry.id === planetId);
    return {
      ...getPlanetEffectiveValues(planetId, attachments, board?.map_tiles, wireBoosts(planetView)),
      traits: meta?.traits ?? [],
      attachments: attachments ?? [],
    };
  };

  const stock = (unit: string, damaged: boolean) =>
    board?.systems[system]?.units.filter(
      (piece) =>
        piece.owner === choice.actor &&
        !piece.planet &&
        piece.unit_type === unit &&
        piece.damaged === damaged,
    ).length ?? 1;

  // Default plan (see presentation/landingPlan.ts): uninhabited planets share the forces, a
  // defended planet keeps the main force. Deterministic: depends only on the offers and the board.
  const computeDefaultPlan = () => {
    const planPlanets: PlanPlanet[] = planets.map((id) => {
      const info = planetInfo(id);
      const meta = board?.map_tiles?.flatMap((tile) => tile.planets ?? []).find((e) => e.id === id);
      const standing = planetStanding(board, system, id, choice.actor).standing;
      return {
        id,
        value: info.resources + info.influence,
        extra: (meta?.legendary ? 10 : 0) + info.attachments.length + (info.traits.length ? 0.5 : 0),
        defended: standing === "defended",
        held: standing === "held",
      };
    });
    const seen = new Set<string>();
    const stocks = options.flatMap(({ landing }) => {
      const key = `${landing.unit}|${landing.damaged}`;
      if (seen.has(key)) return [];
      seen.add(key);
      return [{ unit: landing.unit, damaged: landing.damaged, count: stock(landing.unit, landing.damaged) }];
    });
    return planDefaultLanding(planPlanets, stocks, (planet, unit, damaged) =>
      options.some(
        ({ landing }) => landing.planet === planet && landing.unit === unit && landing.damaged === damaged,
      ),
    );
  };
  const defaultPlan = computeDefaultPlan();
  const defaultTargetPlanet: string | null = defaultPlan.landings[0]?.planet ?? planets[0] ?? null;
  const [planet, setPlanet] = useState<string | null>(defaultTargetPlanet);
  const [hasAutoPopulated, setHasAutoPopulated] = useState(false);
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
    if (oddsPending) {
      setOdds({ key: previewKey, value: "loading" });
      return;
    }
    if (!oddsQuery || !players?.[choice.actor] || !players[opponent]) {
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
      attacker: { faction: players[choice.actor].faction ?? "generic", ...attack },
      defender: {
        faction: players[opponent].faction ?? "generic",
        ...count(defenderForces),
        guns: context.additional_guns,
      },
      harrow: context.harrow_units,
      simulations: 2000,
    };
    let active = true;
    setOdds({ key: previewKey, value: "loading" });
    groundOdds(oddsQuery, request)
      .then((result) => {
        if (active) setOdds({ key: previewKey, value: result.attacker_win_rate });
      })
      .catch((error: unknown) => {
        console.warn("The engine gave no odds for this landing:", error);
        if (active) setOdds({ key: previewKey, value: "unavailable" });
      });
    return () => {
      active = false;
    };
  }, [previewKey, choice.actor, viewerSeat, oddsQuery, oddsPending]);

  useEffect(() => {
    if (!running || submitting.current || !draft.length) return;
    // A hidden tray still holds the last landing offer, whose nonce was answered: check the
    // interruption before the nonce, or the draft resumes on the next offer instead of pausing.
    if (
      interrupted ||
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
    if (submittedNonce.current === choice.nonce) return;
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
  }, [running, draft, interrupted, choice.nonce, choice.actor, choice.context?.subtype, system]);

  useEffect(() => {
    if (running && draft.length === 0) setRunning(false);
  }, [running, draft.length]);

  // Auto-populate the default plan on first load
  useEffect(() => {
    if (
      !hasAutoPopulated &&
      defaultTargetPlanet &&
      draft.length === 0 &&
      localDraft.length === 0 &&
      !controlledDraft?.length
    ) {
      if (defaultPlan.landings.length > 0) {
        setDraft(defaultPlan.landings);
        setPlanet(defaultTargetPlanet);
      }
      setHasAutoPopulated(true);
    }
  }, [defaultTargetPlanet, hasAutoPopulated, draft.length, localDraft.length, controlledDraft?.length]);

  // Reset restores the default plan (including the split)
  const resetToDefaults = () => {
    setDraft(defaultPlan.landings);
    setPlanet(defaultTargetPlanet);
    setError(null);
    setHasAutoPopulated(true);
  };
  const showSplitHint = defaultPlan.split && sameDraft(draft, defaultPlan.landings);

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

            {showSplitHint && (
              <p className="invasion-split-hint" data-testid="invasion-split-hint">
                Split across uninhabited planets — adjust below
                <span className="invasion-split-hint__counts">
                  {" "}
                  (
                  {planets
                    .map((name) => [name, draft.filter((item) => item.planet === name).length] as const)
                    .filter(([, count]) => count > 0)
                    .map(([name, count]) => `${name} ${count}`)
                    .join(", ")}
                  )
                </span>
              </p>
            )}

            <div className="invasion-landing-planets-container">
              {planets.map((name) => {
                const planetUnitsAlready =
                  board?.systems[system]?.units.filter(
                    (piece) => piece.owner === choice.actor && piece.planet === name,
                  ).length ?? 0;
                const planetDraftCount = draft.filter((item) => item.planet === name).length;
                const info = planetInfo(name);
                const isCurrentSelected = planet === name;
                const planetOptions = options.filter(({ landing }) => landing.planet === name);
                const defense = planetDefense(board, system, name, choice.actor, players);

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
                          {name === defaultTargetPlanet && !defaultPlan.split && (
                            <span
                              className="invasion-default-badge"
                              title="Pre-selected as default target"
                              aria-label="default target"
                              style={{
                                display: "inline-block",
                                marginLeft: "4px",
                                fontSize: "10px",
                                backgroundColor: "rgba(255, 193, 7, 0.3)",
                                padding: "2px 6px",
                                borderRadius: "3px",
                                fontWeight: "bold",
                              }}
                            >
                              ★ Default
                            </span>
                          )}
                        </button>
                        <span className="invasion-planet-landing-card__meta">
                          Already on planet: {planetUnitsAlready} · Staged: {planetDraftCount}
                        </span>
                      </div>

                      {defense.forces.length > 0 && (
                        <div className="invasion-defenders" data-testid="invasion-defenders">
                          {defense.forces.map((force) => (
                            <div
                              key={force.owner}
                              className="invasion-defenders__row"
                              data-testid="invasion-defender-force"
                            >
                              <span className="invasion-defenders__owner">
                                {players?.[force.owner]?.faction ?? force.owner}:
                              </span>
                              {force.ground.map((entry) => (
                                <span
                                  key={entry.unit}
                                  className="invasion-defenders__unit invasion-defenders__unit--ground"
                                >
                                  <UnitIcon type={entry.unit} size={14} aria-hidden="true" />
                                  {formatDefenderUnits([entry])}
                                </span>
                              ))}
                              {force.groundTotal === 0 && (
                                <span className="invasion-defenders__unit invasion-defenders__none">
                                  no ground forces
                                </span>
                              )}
                              {force.pds > 0 && (
                                <span className="invasion-defenders__unit invasion-defenders__unit--pds">
                                  <UnitIcon type="pds" size={14} aria-hidden="true" />
                                  {force.pds} PDS
                                </span>
                              )}
                              {force.otherStructures.length > 0 && (
                                <span className="invasion-defenders__unit invasion-defenders__unit--structure">
                                  {formatDefenderUnits(force.otherStructures)}
                                </span>
                              )}
                            </div>
                          ))}
                          {defense.spaceCannonDefense && (
                            <p
                              className={`invasion-defenders__scd${defense.spaceCannonDefense.silencedBy ? " invasion-defenders__scd--silenced" : ""}`}
                              data-testid="invasion-scd"
                            >
                              {defense.spaceCannonDefense.silencedBy
                                ? `Space Cannon Defense silenced by your ${defense.spaceCannonDefense.silencedBy}`
                                : `Space Cannon Defense: ${defense.spaceCannonDefense.dice} ${defense.spaceCannonDefense.dice === 1 ? "die" : "dice"} (${defense.forces
                                    .flatMap((force) => force.guns)
                                    .map((gun) => `${gun.count > 1 ? `${gun.count}× ` : ""}${getUnitDisplayName(gun.unit)} hits on ${gun.hitsOn}`)
                                    .join(", ")}) ${defense.spaceCannonDefense.dice === 1 ? "fires" : "fire"} at forces you land here, ~${defense.spaceCannonDefense.expectedHits} hits`}
                            </p>
                          )}
                        </div>
                      )}

                      <div
                        className="invasion-planet-landing-card__values"
                        data-testid="invasion-planet-values"
                      >
                        <PlanetValue kind="resources" value={info.resources} planetId={name} />
                        <span aria-hidden="true">·</span>
                        <PlanetValue kind="influence" value={info.influence} planetId={name} />
                        {info.traits.length > 0 && (
                          <span className="invasion-planet-trait" title={`Trait: ${info.traits.join(", ")}`}>
                            {info.traits.join(" / ")}
                          </span>
                        )}
                        {info.attachments.length > 0 && (
                          <span
                            className="invasion-planet-trait"
                            title={`Attachments: ${info.attachments.join(", ")}`}
                          >
                            + {info.attachments.join(", ")}
                          </span>
                        )}
                      </div>

                      <ul className="invasion-planet-effects" data-testid="invasion-planet-effects">
                        {planetInvasionInfo(
                          board,
                          system,
                          name,
                          choice.actor,
                          planetDraftCount,
                          (id) => players?.[id]?.faction ?? id,
                        ).effects.map((effect, index) => (
                          <li key={index} className={`invasion-effect invasion-effect--${effect.kind}`}>
                            <span>{effect.text}</span>
                            {effect.detail && (
                              <InfoPopover label={`About: ${effect.text}`} data-testid="planet-effect-info" position="bottom">
                                <p style={{ margin: 0, maxWidth: 280 }}>{effect.detail}</p>
                              </InfoPopover>
                            )}
                          </li>
                        ))}
                      </ul>

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
                onClick={resetToDefaults}
                title="Reset to the default plan: forces split across uninhabited planets, the rest on the contested planet"
              >
                Reset to Defaults
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
        instruction={present(choice.prompt)}
        progress={system ? `Active system: ${system}` : undefined}
        onMinimize={onClose}
      />
      {content}
    </section>
  );
};
