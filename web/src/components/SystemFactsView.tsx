import React from "react";
import { BoardView } from "../protocol/types.ts";
import { describeSystem, planetOccupants } from "../presentation/systemFacts.ts";
import { getPlanetDetails } from "../presentation/planetSelection.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { PlanetValuePair } from "./PlanetValueIcons.tsx";

/** One system as a short fact sheet: planets (R/I), ships present, whose command tokens sit there. */
export const SystemFactsView: React.FC<{ systemId: string; board?: BoardView | null }> = ({
  systemId,
  board,
}) => {
  const present = useParticipantText();
  const facts = describeSystem(systemId, board);
  return (
    <div className="system-facts" data-testid={`system-facts-${systemId}`}>
      <strong className="system-facts__title">{facts.title}</strong>
      <div className="system-facts__line text-muted">
        {facts.planets.length === 0
          ? "No planets"
          : facts.planets.map((p, i) => (
              <React.Fragment key={p.name}>
                {i > 0 && ", "}
                {p.name}
                {p.resources !== null && p.influence !== null && (
                  <>
                    {" ("}
                    <PlanetValuePair resources={p.resources} influence={p.influence} />
                    {")"}
                  </>
                )}
                {p.controlledBy ? ` - ${present(p.controlledBy)}` : ""}
              </React.Fragment>
            ))}
      </div>
      <div className="system-facts__line text-muted">
        {facts.ships.length === 0
          ? "No ships"
          : `Ships: ${facts.ships.map((s) => `${present(s.owner)} ${s.summary}`).join("; ")}`}
        {facts.commandTokens.length > 0 &&
          ` · Command tokens: ${facts.commandTokens.map((o) => present(o)).join(", ")}`}
      </div>
    </div>
  );
};

/** Where a structure goes and what the planet already holds. */
export const StructureInfo: React.FC<{
  planetId: string;
  board?: BoardView | null;
  cost?: string;
}> = ({ planetId, board, cost = "No cost (placed, not built)" }) => {
  const present = useParticipantText();
  const planet = getPlanetDetails(planetId, board);
  const where = planet.systemId ? describeSystem(planet.systemId, board).title : null;
  const there = planetOccupants(planet.systemId, planetId, board);
  return (
    <div className="system-facts" data-testid="structure-info">
      <div className="system-facts__line">
        <span data-testid="structure-info-where">
          {planet.name}
          {where ? ` in ${where}` : ""}
        </span>
        {" · "}
        <span data-testid="structure-info-cost">{cost}</span>
      </div>
      <div className="system-facts__line text-muted" data-testid="structure-info-there">
        {there.length === 0
          ? "Nothing on the planet yet"
          : `Already there: ${there.map((o) => `${present(o.owner)} ${o.summary}`).join("; ")}`}
      </div>
    </div>
  );
};
