import React from "react";
import type { PlacedUnitPresentation } from "../presentation/boardPresentation.ts";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { useSeatInfo } from "../presentation/SeatInfoContext.tsx";
import {
  groupStructures,
  splitStructures,
  structureCard,
  type StructureGroup,
} from "../presentation/planetStructures.ts";
import { StructureIcon } from "./StructureIcon.tsx";
import { getUnitDisplayName } from "./UnitIcon.tsx";

const Row: React.FC<{ planetId: string; group: StructureGroup }> = ({ planetId, group }) => {
  const display = usePlayerIdentity();
  const { faction, technologies } = useSeatInfo(group.owner);
  const who = display(group.owner);
  const card = structureCard(group.unitType, faction, technologies);
  const name = card?.name ?? getUnitDisplayName(group.unitType, 1);
  const count = group.count > 1 ? `${group.count} × ` : "";
  const label = `${count}${name}, ${who.label}${group.damaged > 0 ? ", damaged" : ""}`;
  return (
    <div
      data-testid={`planet-structure-${planetId}-${group.kind}`}
      data-owner={group.owner}
      title={label}
      style={{
        display: "flex",
        flexWrap: "wrap",
        alignItems: "center",
        gap: "2px 6px",
        fontSize: 12,
        color: group.damaged > 0 ? "#fca5a5" : "#e2e8f0",
        borderLeft: `3px solid ${group.ownerColor}`,
        paddingLeft: 6,
      }}
    >
      {who.position && <SeatBadge position={who.position} />}
      <StructureIcon kind={group.kind} size={20} color={group.ownerColor} label={label} />
      <span style={{ fontWeight: 600 }}>
        {count}
        {name}
      </span>
      {card?.details.map((d) => (
        <span key={d} style={{ color: "#94a3b8" }}>
          {d}
        </span>
      ))}
      {group.damaged > 0 && (
        <span style={{ color: "#f87171", fontWeight: 600 }}>({group.damaged} damaged)</span>
      )}
    </div>
  );
};

/** PDS and space docks of one planet, inside its card. Renders nothing without structures. */
export const PlanetStructures: React.FC<{
  planetId: string;
  units: readonly PlacedUnitPresentation[];
}> = ({ planetId, units }) => {
  const groups = groupStructures(splitStructures(units).structures);
  if (groups.length === 0) return null;
  return (
    <div
      data-testid={`planet-structures-${planetId}`}
      style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 4 }}
    >
      {groups.map((g) => (
        <Row key={`${g.owner}|${g.unitType}`} planetId={planetId} group={g} />
      ))}
    </div>
  );
};
