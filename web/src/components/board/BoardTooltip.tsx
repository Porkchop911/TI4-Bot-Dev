import React from "react";
import { getPlayerColor, TilePresentation } from "../../presentation/boardPresentation.ts";
import {
  MapOverlayMode,
  computeTileEconomy,
  computeTileSpaceCombat,
  computeTileGroundCombat,
  computeTileTechBenefits,
} from "../../presentation/mapOverlays.ts";
import { SeatBadge, usePlayerIdentity } from "../../presentation/PlayerIdentity.tsx";
import { EconomyTooltipSection } from "./EconomyOverlay.tsx";
import { SpaceCombatTooltipSection } from "./SpaceCombatOverlay.tsx";
import { GroundCombatTooltipSection } from "./GroundCombatOverlay.tsx";
import { TechBenefitsTooltipSection } from "./TechBenefitsOverlay.tsx";
import { UnitStatLine } from "../UnitInfo.tsx";
import { getUnitDisplayName } from "../UnitIcon.tsx";
import { describeAnomaly } from "../../presentation/anomalyRules.ts";
import type { PlanetBoost } from "../../presentation/planetValues.ts";
import { PlanetValuePair } from "../PlanetValueIcons.tsx";

export interface HoveredTileInfo {
  systemId: string;
  label: string;
  anomalies?: string[];
  wormholes?: string[];
  planets: { label: string; resources?: number; influence?: number; boosts?: PlanetBoost[]; owner?: string | null }[];
  unitsCount: number;
  tokensCount: number;
}

import { PlayerView } from "../../protocol/types.ts";

export interface BoardTooltipProps {
  hoveredTile: HoveredTileInfo;
  tilePresentation?: TilePresentation;
  activeOverlay: MapOverlayMode;
  seatingOrder: string[];
  players?: readonly PlayerView[];
}

export const BoardTooltip: React.FC<BoardTooltipProps> = ({
  hoveredTile,
  tilePresentation,
  activeOverlay,
  seatingOrder,
  players,
}) => {
  const display = usePlayerIdentity();

  const eco =
    tilePresentation && activeOverlay === "economy" ? computeTileEconomy(tilePresentation) : null;
  const combat =
    tilePresentation && activeOverlay === "space_combat"
      ? computeTileSpaceCombat(tilePresentation, players)
      : null;
  const ground =
    tilePresentation && activeOverlay === "ground_combat"
      ? computeTileGroundCombat(tilePresentation, players)
      : null;
  const tech =
    tilePresentation && activeOverlay === "tech_benefits"
      ? computeTileTechBenefits(tilePresentation)
      : null;

  return (
    <div
      role="tooltip"
      aria-live="polite"
      data-testid="system-tooltip"
      className="board-tooltip panel"
      style={{
        border: "1px solid #38bdf8",
        padding: "10px 14px",
        fontSize: 13,
        pointerEvents: "none",
        maxWidth: 320,
      }}
    >
      <div style={{ fontWeight: "bold", color: "#38bdf8", marginBottom: 4 }}>
        System #{hoveredTile.systemId} — {hoveredTile.label}
      </div>
      {hoveredTile.anomalies && hoveredTile.anomalies.length > 0 && (
        <div style={{ color: "#fef08a", fontSize: 12 }}>
          Anomaly: {hoveredTile.anomalies.join(", ")}
          {hoveredTile.anomalies.map((a) => {
            const rule = describeAnomaly(a);
            return rule ? (
              <div key={a} data-testid="tooltip-anomaly-rule" style={{ fontSize: 11, marginTop: 2 }}>
                {rule.text}
              </div>
            ) : null;
          })}
        </div>
      )}
      {hoveredTile.wormholes && hoveredTile.wormholes.length > 0 && (
        <div style={{ color: "#a78bfa", fontSize: 12 }}>
          Wormholes: {hoveredTile.wormholes.join(", ")}
        </div>
      )}
      <div style={{ marginTop: 4 }}>
        <span style={{ fontWeight: "bold", color: "#cbd5e1" }}>Planets: </span>
        {hoveredTile.planets.length > 0 ? (
          hoveredTile.planets.map((p, i) => (
            <div key={i} style={{ marginLeft: 6, fontSize: 12 }}>
              • {p.label}
              {p.resources !== undefined && p.influence !== undefined && (
                <>
                  {" "}
                  <PlanetValuePair resources={p.resources} influence={p.influence} boosts={p.boosts} size="tooltip" />
                </>
              )}
              {p.owner && (
                <span style={{ color: getPlayerColor(p.owner, seatingOrder) }}>
                  {" "}
                  [
                  {display(p.owner).position && (
                    <SeatBadge position={display(p.owner).position!} />
                  )}{" "}
                  {display(p.owner).label}]
                </span>
              )}
            </div>
          ))
        ) : (
          <span style={{ fontSize: 12, color: "#94a3b8" }}>None</span>
        )}
      </div>
      <div style={{ marginTop: 4, fontSize: 12 }}>
        Units: {hoveredTile.unitsCount} | Command Tokens: {hoveredTile.tokensCount}
      </div>
      {tilePresentation && tilePresentation.units.length > 0 && (
        <TileUnitStats units={tilePresentation.units} />
      )}

      {/* Overlay-specific Tooltip Sections */}
      {eco && <EconomyTooltipSection eco={eco} />}
      {combat && <SpaceCombatTooltipSection combat={combat} />}
      {ground && <GroundCombatTooltipSection ground={ground} />}
      {tech && <TechBenefitsTooltipSection tech={tech} />}
    </div>
  );
};

/** Each owner's unit types on the tile with their numbers, so a rival's fleet can be read by hovering. */
const TileUnitStats: React.FC<{ units: TilePresentation["units"] }> = ({ units }) => {
  const display = usePlayerIdentity();
  const rows = new Map<string, { owner: string; type: string; count: number }>();
  for (const unit of units) {
    const key = `${unit.owner}|${unit.unitType}`;
    const row = rows.get(key) ?? { owner: unit.owner, type: unit.unitType, count: 0 };
    row.count += 1;
    rows.set(key, row);
  }
  // One block per owner (first seen first), so two players' units are never interleaved.
  const owners = [...new Set([...rows.values()].map((row) => row.owner))];
  return (
    <div data-testid="tooltip-unit-stats" style={{ marginTop: 6, fontSize: 11, color: "#cbd5e1" }}>
      {owners.map((owner) => (
        <div key={owner} data-testid={`tooltip-owner-${owner}`} style={{ marginTop: 4 }}>
          <strong>{display(owner).label}</strong>
          {[...rows.values()]
            .filter((row) => row.owner === owner)
            .map((row) => (
              <div key={row.type} style={{ marginTop: 2, marginLeft: 6 }}>
                <span>
                  {row.count} × {getUnitDisplayName(row.type, row.count)}
                </span>
                <div style={{ color: "#94a3b8" }}>
                  <UnitStatLine unit={row.type} seat={owner} />
                </div>
              </div>
            ))}
        </div>
      ))}
    </div>
  );
};
