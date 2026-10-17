import React from "react";
import { PlanetValuePair, ValueUnit } from "./PlanetValueIcons.tsx";
import { spenderAnswer, type ReadyHints } from "../presentation/readyHints.ts";
import "./ReadyHintsPanel.css";

/**
 * Help for the "ready planets?" decision: which exhausted planets could be readied, and whether
 * the strategy cards that spend resources or influence (Leadership, Technology, Warfare) have
 * already been played this round, so waiting for them can be weighed against readying now.
 */
export const ReadyHintsPanel: React.FC<{ hints: ReadyHints; showPlanets?: boolean }> = ({
  hints,
  showPlanets = true,
}) => (
  <div className="ready-hints" data-testid="ready-hints">
    {showPlanets && (
      <div className="ready-hints__block" data-testid="ready-hints-planets">
        <div className="ready-hints__title">Planets you could ready</div>
        {hints.planets.length === 0 ? (
          <div className="ready-hints__empty text-muted">None: nothing of yours is exhausted</div>
        ) : (
          <ul className="ready-hints__planets">
            {hints.planets.map((planet) => (
              <li key={planet.id} data-testid={`ready-hint-planet-${planet.id}`}>
                <span className="ready-hints__name">{planet.name}</span>
                {planet.resources !== null && planet.influence !== null && (
                  <PlanetValuePair resources={planet.resources} influence={planet.influence} planetId={planet.id} />
                )}
                <span className="ready-hints__tag">exhausted</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    )}
    <div className="ready-hints__block" data-testid="ready-hints-spenders">
      <div className="ready-hints__title">Spending cards this round</div>
      <ul className="ready-hints__spenders">
        {hints.spenders.map((spender) => {
          const answer = spenderAnswer(spender.status);
          return (
            <li
              key={spender.card}
              data-testid={`ready-hint-${spender.card.toLowerCase()}`}
              data-played={answer.played}
            >
              <span>
                {spender.card} <ValueUnit kind={spender.spends} /> played:
              </span>{" "}
              <strong className={answer.played ? "ready-hints__yes" : "ready-hints__no"}>
                {answer.text}
              </strong>
            </li>
          );
        })}
      </ul>
    </div>
  </div>
);
