import React from "react";
import type { UnitCount } from "../presentation/combatSummary.ts";
import type { GroundCombatSummary, GroundSideSummary } from "../presentation/groundCombatSummary.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { getUnitDisplayName } from "./UnitIcon.tsx";

const list = (counts: UnitCount[], none: string) =>
  counts.length
    ? counts.map(({ unit, count }) => `${count} × ${getUnitDisplayName(unit, count)}`).join(", ")
    : none;

const Side: React.FC<{ summary: GroundSideSummary; role: string }> = ({ summary, role }) => {
  const display = usePlayerIdentity();
  return (
    <div className="combat-result-side" data-testid={`ground-result-${role}`}>
      <div className="combat-result-side__name">
        <span className="combat-fleet-card__role">{role.toUpperCase()}</span> {display(summary.seat).label}
      </div>
      <dl className="combat-result-side__facts">
        <dt>Before</dt>
        <dd>{list(summary.before, "No ground forces")}</dd>
        <dt>Hits scored</dt>
        <dd>{summary.hits ?? "–"}</dd>
        <dt>Lost</dt>
        <dd>{list(summary.lost, "No units")}</dd>
        {summary.sustained.length > 0 && (
          <>
            <dt>Sustained damage</dt>
            <dd>{list(summary.sustained, "")}</dd>
          </>
        )}
        <dt>After</dt>
        <dd>{list(summary.after, "None left")}</dd>
      </dl>
    </div>
  );
};

/** The outcome of a finished ground combat: who invaded and defended which planet, and what is next. */
export const GroundCombatResultSummary: React.FC<{ summary: GroundCombatSummary }> = ({ summary }) => {
  const display = usePlayerIdentity();
  const { verdict, planet } = summary;
  const headline =
    verdict.kind === "attacker_won"
      ? `${display(verdict.winner).label} conquered ${planet}`
      : verdict.kind === "defender_held"
        ? `${display(verdict.winner).label} held ${planet}`
        : `Mutual destruction on ${planet}`;
  const next =
    verdict.kind === "attacker_won"
      ? `${display(verdict.loser).label}'s ground forces were wiped out. Control of the planet passes to ${display(verdict.winner).label}.`
      : verdict.kind === "defender_held"
        ? `The invasion of ${planet} failed. Control of the planet stays with ${display(verdict.winner).label}.`
        : `No ground forces remain; control of ${planet} does not change.`;
  return (
    <section
      className="combat-result-summary"
      data-testid="ground-combat-result-summary"
      aria-label="Ground combat result"
    >
      <h3 className="combat-result-summary__headline" data-testid="ground-result-headline">{headline}</h3>
      <p className="combat-result-summary__detail" data-testid="ground-result-detail">
        {next} Summary of round {summary.round}.
      </p>
      <div className="combat-result-summary__sides">
        <Side summary={summary.attacker} role="attacker" />
        <Side summary={summary.defender} role="defender" />
      </div>
    </section>
  );
};
