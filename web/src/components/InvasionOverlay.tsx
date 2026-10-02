import React, { useRef } from "react";
import type { BoardView, PendingChoiceDto, PlacedUnitView, PlayerView } from "../protocol/types.ts";
import { findExplorationCardMeta, findActionCardMeta } from "../protocol/contentCatalog.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { InvasionLandingTray, type Landing } from "./InvasionLandingTray.tsx";
import { UnitIcon, getUnitDisplayName } from "./UnitIcon.tsx";
import { WorkflowShell } from "./WorkflowShell.tsx";

export interface InvasionEffectInfo {
  typeClass: string;
  icon: string;
  categoryLabel: string;
  title: string;
  description: string;
  flavorText?: string;
}

export function getInvasionEffectInfo(
  choice: PendingChoiceDto,
  currentPlanet?: string | null,
  groundRound?: number,
): InvasionEffectInfo {
  // 1. Check if it's an exploration card (by prompt, subtype, or explore option)
  const exploreMeta =
    findExplorationCardMeta(choice.prompt) ||
    (choice.context?.subtype ? findExplorationCardMeta(choice.context.subtype) : undefined) ||
    (choice.options.some((o) => o.kind === "explore")
      ? findExplorationCardMeta(
          (choice.options.find((o) => typeof o.payload?.card === "string")?.payload
            ?.card as string) ?? "",
        )
      : undefined);

  if (exploreMeta) {
    const trait = exploreMeta.type.toLowerCase();
    const icons: Record<string, string> = {
      industrial: "🏭",
      hazardous: "☣️",
      cultural: "🏛️",
      frontier: "🌌",
    };
    return {
      typeClass: trait,
      icon: icons[trait] ?? "🪐",
      categoryLabel: `Planet Exploration · ${exploreMeta.type}`,
      title: exploreMeta.name,
      description: exploreMeta.description,
      flavorText: exploreMeta.flavorText,
    };
  }

  // 2. Check if it's an action card
  const cardPayload = choice.options.find((o) => typeof o.payload?.card === "string")?.payload
    ?.card as string | undefined;
  const cardId = cardPayload ?? choice.context?.subtype;
  const actionCardMeta = cardId ? findActionCardMeta(cardId) : undefined;
  if (actionCardMeta) {
    return {
      typeClass: "action",
      icon: "⚡",
      categoryLabel: "Action Card · Reaction",
      title: actionCardMeta.name,
      description: actionCardMeta.description,
    };
  }

  // 3. Check if it's ground combat round
  if (choice.context?.subtype === "fight_ground_combat_round") {
    return {
      typeClass: "combat",
      icon: "⚔️",
      categoryLabel: "Ground Combat",
      title: `Ground Battle${currentPlanet ? ` · ${currentPlanet}` : ""}${groundRound ? ` · Round ${groundRound}` : ""}`,
      description: `Roll combat dice for all attacking and defending ground forces${currentPlanet ? ` on ${currentPlanet}` : ""}.`,
    };
  }

  // 4. Fallback general invasion choice
  return {
    typeClass: "general",
    icon: "🪐",
    categoryLabel: "Planet Invasion Effect",
    title: choice.prompt,
    description: "",
  };
}

export const InvasionEffectCard: React.FC<{
  choice: PendingChoiceDto;
  currentPlanet?: string | null;
  groundRound?: number;
  isDirectSubmitting?: boolean;
  onSubmit?: (id: string) => Promise<void> | void;
  waitingFor?: string;
}> = ({ choice, currentPlanet, groundRound, isDirectSubmitting, onSubmit, waitingFor }) => {
  const info = getInvasionEffectInfo(choice, currentPlanet, groundRound);

  return (
    <div
      className={`invasion-effect-card invasion-effect-card--${info.typeClass}`}
      data-testid="invasion-effect-card"
    >
      <div className="invasion-effect-card__header">
        <div className="invasion-effect-card__meta">
          <span className={`invasion-effect-badge invasion-effect-badge--${info.typeClass}`}>
            <span className="invasion-effect-badge__icon" aria-hidden="true">
              {info.icon}
            </span>
            {info.categoryLabel}
          </span>
          {currentPlanet && (
            <span className="invasion-effect-badge invasion-effect-badge--planet">
              🪐 {currentPlanet}
            </span>
          )}
        </div>
        <h3 className="invasion-effect-card__title">{info.title}</h3>
      </div>

      {info.description ? (
        <p className="invasion-effect-card__description">{info.description}</p>
      ) : (
        choice.prompt !== info.title && (
          <p className="invasion-effect-card__description">{choice.prompt}</p>
        )
      )}

      {info.flavorText && (
        <blockquote className="invasion-effect-card__flavor">{info.flavorText}</blockquote>
      )}

      {waitingFor ? (
        <div className="invasion-effect-card__waiting">
          <span className="invasion-spinner-dot" aria-hidden="true" />
          <span>
            Waiting for <strong>{waitingFor}</strong> to choose…
          </span>
        </div>
      ) : (
        <div className="decision-frame__options invasion-effect-card__options">
          {choice.options.map((option, index) => {
            const isDecline =
              option.kind === "decline" || option.id === "decline" || option.id === "pass";
            const isPrimary = index === 0 && !isDecline;
            return (
              <button
                key={option.id}
                type="button"
                className={`button ${isPrimary ? "button--primary invasion-effect-btn--primary" : "button--secondary invasion-effect-btn--secondary"} invasion-effect-btn`}
                disabled={isDirectSubmitting}
                onClick={() => void onSubmit?.(option.id)}
              >
                <span className="invasion-effect-btn__label">
                  {choice.context?.subtype === "fight_ground_combat_round"
                    ? "Fight next round"
                    : option.label}
                </span>
                {option.description && (
                  <small className="invasion-effect-btn__sub"> · {option.description}</small>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
};

export const InvasionOverlay: React.FC<{
  board: BoardView;
  players?: Record<string, PlayerView>;
  choice: PendingChoiceDto | null;
  viewerSeat?: string | null;
  onSubmit: (id: string) => Promise<void>;
  onClose: () => void;
  lastError?: string | null;
  landingDraft?: Landing[];
  onLandingDraftChange?: (draft: Landing[]) => void;
}> = ({
  board,
  choice,
  players,
  viewerSeat,
  onSubmit,
  onClose,
  lastError,
  landingDraft,
  onLandingDraftChange,
}) => {
  const display = usePlayerIdentity();
  const lastLandingChoice = useRef<PendingChoiceDto | null>(null);
  const invasion = board.invasion;
  if (!invasion) return null;

  const isLandingChoice =
    choice?.context?.subtype === "commit_ground_forces" && choice.actor === invasion.invader;
  if (isLandingChoice) {
    lastLandingChoice.current = choice;
  }
  const trayChoice = isLandingChoice ? choice : lastLandingChoice.current;
  const system = board.systems[invasion.system_id];
  const step = invasion.last_step;
  const invaderDisplay = display(invasion.invader);

  const describe = (units: PlacedUnitView[]) =>
    units
      .map((unit) => `${unit.owner} ${unit.unit_type}${unit.damaged ? " (damaged)" : ""}`)
      .join(", ") || "None";

  return (
    <section
      className="panel decision-frame invasion-overlay"
      data-testid="invasion-overlay"
      aria-label="Invasion"
    >
      <header className="invasion-overlay__header">
        <div className="invasion-overlay__header-main">
          <div className="invasion-overlay__badges">
            <span className="invasion-badge invasion-badge--phase">⚔️ INVASION</span>
            <span className="invasion-badge invasion-badge--system">
              System {invasion.system_id}
            </span>
            <span className="invasion-badge invasion-badge--status">
              {invasion.phase.replaceAll("_", " ")}
            </span>
          </div>
          <h2 className="invasion-overlay__title">Invasion · {invasion.system_id}</h2>
          <p className="invasion-overlay__subtitle">
            <span className="invasion-invader-label">
              <span
                className="invasion-player-dot"
                style={{ backgroundColor: invaderDisplay.color ?? undefined }}
              />
              <strong>{invaderDisplay.label}</strong>
              {players?.[invasion.invader]?.faction && (
                <span className="text-muted"> ({players[invasion.invader].faction})</span>
              )}
            </span>
            <span className="invasion-sep">·</span>
            <span className="invasion-phase-text">{invasion.phase.replaceAll("_", " ")}</span>
          </p>
        </div>
        <button
          type="button"
          className="button button--secondary button--icon invasion-overlay__close-btn"
          aria-label="Minimize decision"
          title="Minimize decision"
          onClick={onClose}
        >
          −
        </button>
      </header>

      {!isLandingChoice && invasion.planets.length > 0 && (
        <nav aria-label="Invasion planets" className="invasion-planets-nav">
          {invasion.planets.map((planet) => {
            const controller = system?.planets[planet]?.controlled_by;
            const ctrlDisplay = controller ? display(controller) : null;
            const isCurrent = planet === invasion.current_planet;
            return (
              <span
                key={planet}
                className={`card invasion-planet-chip ${isCurrent ? "card--selected invasion-planet-chip--selected" : ""}`}
              >
                <span className="invasion-planet-chip__name">🪐 {planet}</span>
                <span className="invasion-planet-chip__sep">·</span>
                <span className="invasion-planet-chip__owner">
                  {ctrlDisplay ? (
                    <>
                      <span
                        className="invasion-planet-chip__dot"
                        style={{ backgroundColor: ctrlDisplay.color ?? undefined }}
                      />
                      {ctrlDisplay.label}
                    </>
                  ) : (
                    (controller ?? "uncontrolled")
                  )}
                </span>
                {isCurrent && <span className="invasion-planet-chip__target-tag">Target</span>}
              </span>
            );
          })}
        </nav>
      )}

      {invasion.current_planet && (
        <div className="invasion-current-planet-banner">
          <span className="invasion-current-planet-banner__item">
            Current planet: <strong>{invasion.current_planet}</strong>
          </span>
          <span className="invasion-sep">·</span>
          <span className="invasion-current-planet-banner__item">
            Defender:{" "}
            <strong>{invasion.defender ? display(invasion.defender).label : "none"}</strong>
          </span>
          <span className="invasion-sep">·</span>
          <span className="invasion-current-planet-banner__item">
            Ground round: <strong>{invasion.ground_round}</strong>
          </span>
        </div>
      )}

      {step && (
        <section
          aria-label={`${step.planet} ${step.kind} result`}
          data-testid="invasion-step"
          className="invasion-step-card"
        >
          <div className="invasion-step-card__header">
            <span className="invasion-step-card__badge">ROUND RESULT</span>
            <h3 className="invasion-step-card__title">
              {step.planet} · {step.kind.replaceAll("_", " ")}{" "}
              {step.round > 0 ? `· round ${step.round}` : ""}
            </h3>
          </div>
          <div className="invasion-step-card__body">
            <p className="invasion-step-card__forces">Before: {describe(step.before)}</p>
            <div className="invasion-step-card__hits">
              <span className="invasion-step-card__hits-label">Hits: </span>
              <span className="invasion-step-card__hits-value">
                {Object.entries(step.hits)
                  .map(([owner, hits]) => `${display(owner).label} ${hits}`)
                  .join(" · ")}
                {step.harrow_hits > 0 ? ` · Harrow ${step.harrow_hits}` : ""}
              </span>
            </div>
            {step.dice.length > 0 && (
              <div aria-label="Ground dice" className="invasion-step-card__dice">
                {step.dice.map((die, index) => (
                  <span
                    key={index}
                    className={`card invasion-die-chip ${die.hit ? "invasion-die-chip--hit" : "invasion-die-chip--miss"}`}
                  >
                    <span className="invasion-die-chip__icon">{die.hit ? "💥" : "⚪"}</span>
                    <span className="invasion-die-chip__text">
                      {die.player} · {die.group}: {die.face} / {die.target}{" "}
                      {die.hit ? "hit" : "miss"}
                    </span>
                  </span>
                ))}
              </div>
            )}
            <p className="invasion-step-card__forces">After: {describe(step.after)}</p>
          </div>
        </section>
      )}

      {!isLandingChoice && invasion.planets.length > 0 && (
        <div className="invasion-planets-grid">
          {invasion.planets.map((planet) => {
            const planetUnits = system?.units.filter((unit) => unit.planet === planet) ?? [];
            return (
              <div key={planet} className="invasion-planet-card">
                <div className="invasion-planet-card__header">
                  <h3 className="invasion-planet-card__name">🪐 {planet}</h3>
                  <span className="invasion-planet-card__count">
                    {planetUnits.length} force{planetUnits.length === 1 ? "" : "s"}
                  </span>
                </div>
                <div className="invasion-planet-card__forces">
                  {planetUnits.length > 0 ? (
                    <div className="invasion-unit-chips">
                      {planetUnits.map((unit, idx) => {
                        const ownerDisplay = display(unit.owner);
                        return (
                          <span key={idx} className="invasion-unit-chip">
                            <span
                              className="invasion-unit-chip__dot"
                              style={{ backgroundColor: ownerDisplay.color ?? undefined }}
                            />
                            <UnitIcon type={unit.unit_type} />
                            <span className="invasion-unit-chip__name">
                              {ownerDisplay.label} {getUnitDisplayName(unit.unit_type)}
                            </span>
                            {unit.damaged && (
                              <span className="invasion-unit-chip__damaged">damaged</span>
                            )}
                          </span>
                        );
                      })}
                    </div>
                  ) : (
                    <p className="invasion-planet-card__empty text-muted">No forces on planet</p>
                  )}
                  <p className="visually-hidden">
                    {planetUnits
                      .map(
                        (unit) =>
                          `${unit.owner} ${unit.unit_type}${unit.damaged ? " (damaged)" : ""}`,
                      )
                      .join(", ") || "No forces on planet"}
                  </p>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {viewerSeat === invasion.invader &&
      landingDraft?.length &&
      (!isLandingChoice || choice?.actor !== viewerSeat) ? (
        <div className="invasion-draft-banner" data-testid="invasion-interrupted-draft">
          <span className="invasion-draft-banner__icon">⏸️</span>
          <div className="invasion-draft-banner__text">
            <strong>Landing paused</strong> · remaining draft:{" "}
            {landingDraft
              .map((item) => `${item.unit}${item.damaged ? " (damaged)" : ""} → ${item.planet}`)
              .join(", ")}
          </div>
        </div>
      ) : null}

      {trayChoice && viewerSeat === invasion.invader && (
        <div hidden={!isLandingChoice || choice?.actor !== viewerSeat}>
          <InvasionLandingTray
            embedded
            choice={trayChoice}
            board={board}
            players={players}
            viewerSeat={viewerSeat}
            onSubmit={onSubmit}
            onClose={onClose}
            lastError={lastError}
            draft={landingDraft}
            onDraftChange={onLandingDraftChange}
          />
        </div>
      )}

      {choice && isLandingChoice && viewerSeat === choice.actor ? null : choice &&
        viewerSeat === choice.actor ? (
        <WorkflowShell
          choice={choice}
          viewerSeat={viewerSeat}
          onSubmit={onSubmit}
          lastError={lastError}
          errorTestId="invasion-error"
        >
          {({ isDirectSubmitting, submitDirect }) => (
            <InvasionEffectCard
              choice={choice}
              currentPlanet={invasion.current_planet}
              groundRound={invasion.ground_round}
              isDirectSubmitting={isDirectSubmitting}
              onSubmit={submitDirect}
            />
          )}
        </WorkflowShell>
      ) : (
        <div className="invasion-waiting-wrap">
          {choice ? (
            <InvasionEffectCard
              choice={choice}
              currentPlanet={invasion.current_planet}
              groundRound={invasion.ground_round}
              waitingFor={display(choice.actor).label}
            />
          ) : (
            <p className="invasion-waiting-note">Waiting for invasion resolution</p>
          )}
        </div>
      )}
    </section>
  );
};
