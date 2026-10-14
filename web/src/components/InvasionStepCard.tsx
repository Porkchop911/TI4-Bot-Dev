import React from "react";
import type { InvasionStepView } from "../protocol/types.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import {
  formatCounts,
  planetName,
  shooterLabel,
  STEP_TITLES,
  stepOutcomeText,
  summarizeInvasionStep,
} from "../presentation/invasionSteps.ts";
import { UnitIcon } from "./UnitIcon.tsx";

const WHAT: Record<"bombardment" | "space_cannon_defense", string> = {
  bombardment:
    "Bombardment: the invader's ships fire at the ground forces on the planet. Hits destroy ground forces only (never structures); a Mech may sustain damage to cancel one.",
  space_cannon_defense:
    "Space Cannon Defense: after the landing, the defender's Space Cannon units (such as a PDS) fire at the ground forces that just landed. Hits destroy landed ground forces.",
};

/**
 * The result of a bombardment or Space Cannon Defense step: who fired what, every die with its
 * hit threshold, and which ground forces were lost. Seats and units are shown by name, never by id.
 */
export const InvasionStepCard: React.FC<{ step: InvasionStepView; invader: string }> = ({ step, invader }) => {
  const display = usePlayerIdentity();
  const summary = summarizeInvasionStep(step, invader);
  if (!summary) return null;
  const planet = planetName(summary.planet);
  const title = STEP_TITLES[summary.kind];
  const outcome = stepOutcomeText(summary, invader, (seat) => display(seat).label);
  return (
    <section
      aria-label={`${planet} ${title} result`}
      data-testid="invasion-step"
      data-step-kind={summary.kind}
      className="invasion-step-card invasion-step-card--defense"
    >
      <div className="invasion-step-card__header">
        <span className="invasion-step-card__badge">{title.toUpperCase()}</span>
        <h3 className="invasion-step-card__title">{planet}</h3>
      </div>
      <p className="invasion-step-card__what text-muted">{WHAT[summary.kind]}</p>
      {summary.shooters.length > 0 ? (
        <ul className="invasion-shooters" aria-label="Dice rolled">
          {summary.shooters.map((group) => (
            <li key={`${group.seat}|${group.unit}|${group.target}`} className="invasion-shooter">
              <div className="invasion-shooter__head">
                <UnitIcon type={group.unit} size={18} />
                <strong>{display(group.seat).label}</strong>
                <span>{shooterLabel(summary.kind, group)}</span>
                <span className="invasion-shooter__hits">
                  {group.hits} hit{group.hits === 1 ? "" : "s"}
                </span>
              </div>
              <div className="invasion-step-card__dice">
                {group.faces.map((die, index) => (
                  <span
                    key={index}
                    className={`card invasion-die-chip ${die.hit ? "invasion-die-chip--hit" : "invasion-die-chip--miss"}`}
                    title={`${die.face} rolled, needs ${group.target} or more: ${die.hit ? "hit" : "miss"}`}
                  >
                    <span className="invasion-die-chip__icon" aria-hidden="true">
                      {die.hit ? "💥" : "⚪"}
                    </span>
                    <span className="invasion-die-chip__text">
                      {die.face} <span className="text-muted">/ {group.target}+</span> {die.hit ? "hit" : "miss"}
                    </span>
                  </span>
                ))}
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <p className="invasion-step-card__forces">No dice were rolled.</p>
      )}
      <p className="invasion-step-card__result" data-testid="invasion-step-outcome">
        <strong>Result:</strong> {outcome}
      </p>
      <ul className="invasion-step-card__effects" aria-label="Ground forces affected">
        {summary.targets.map((side) => (
          <li key={side.seat}>
            <strong>{display(side.seat).label}</strong> had {formatCounts(side.before)}
            {side.lost.length > 0 && <> · lost <strong>{formatCounts(side.lost)}</strong></>}
            {side.sustained.length > 0 && <> · {formatCounts(side.sustained)} sustained damage</>}
            {side.lost.length === 0 && side.sustained.length === 0 && <> · nothing lost</>}
            {" · "}
            {side.remaining} ground force{side.remaining === 1 ? "" : "s"} left
          </li>
        ))}
        {summary.targets.length === 0 && <li>No ground forces were on the planet to hit.</li>}
      </ul>
    </section>
  );
};
