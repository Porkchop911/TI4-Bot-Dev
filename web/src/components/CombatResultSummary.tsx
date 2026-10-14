import React from "react";
import type { CombatSideSummary, CombatSummary, UnitCount } from "../presentation/combatSummary.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { getUnitDisplayName } from "./UnitIcon.tsx";

const list = (counts: UnitCount[]) =>
  counts.map(({ unit, count }) => `${count} × ${getUnitDisplayName(unit, count)}`).join(", ");

const Side: React.FC<{ summary: CombatSideSummary; role: string }> = ({ summary, role }) => {
  const display = usePlayerIdentity();
  return (
    <div className="combat-result-side" data-testid={`combat-result-${role}`}>
      <div className="combat-result-side__name">
        <span className="combat-fleet-card__role">{role.toUpperCase()}</span> {display(summary.seat).label}
      </div>
      <dl className="combat-result-side__facts">
        <dt>Hits scored</dt>
        <dd>{summary.hits ?? "–"}</dd>
        <dt>Lost</dt>
        <dd>{summary.lost.length ? list(summary.lost) : "No ships"}</dd>
        {summary.sustained.length > 0 && (
          <>
            <dt>Sustained damage</dt>
            <dd>{list(summary.sustained)}</dd>
          </>
        )}
        <dt>Ships left</dt>
        <dd>{summary.remaining}</dd>
      </dl>
    </div>
  );
};

/** The outcome of a finished space combat: who won, and what the last round cost each side. */
export const CombatResultSummary: React.FC<{ summary: CombatSummary }> = ({ summary }) => {
  const display = usePlayerIdentity();
  const { verdict } = summary;
  const headline =
    verdict.kind === "won"
      ? `${display(verdict.winner).label} won the battle`
      : verdict.kind === "annihilated"
        ? "Both fleets were destroyed"
        : "Both fleets survive";
  const detail =
    verdict.kind === "won"
      ? `${display(verdict.loser).label}'s fleet was destroyed.`
      : verdict.kind === "both_remain"
        ? "The combat ended with ships of both sides in the system."
        : "No ships remain in the system.";
  return (
    <section className="combat-result-summary" data-testid="combat-result-summary" aria-label="Combat result">
      <h3 className="combat-result-summary__headline" data-testid="combat-result-headline">{headline}</h3>
      <p className="combat-result-summary__detail">{detail} Summary of round {summary.round}.</p>
      <div className="combat-result-summary__sides">
        <Side summary={summary.attacker} role="attacker" />
        <Side summary={summary.defender} role="defender" />
      </div>
    </section>
  );
};
