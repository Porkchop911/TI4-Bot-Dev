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

export interface HoveredTileInfo {
  systemId: string;
  label: string;
  anomalies?: string[];
  wormholes?: string[];
  planets: { label: string; resources?: number; influence?: number; owner?: string | null }[];
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
              {p.resources !== undefined && ` (${p.resources} Res / ${p.influence} Inf)`}
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

      {/* Overlay-specific Tooltip Sections */}
      {eco && <EconomyTooltipSection eco={eco} />}
      {combat && <SpaceCombatTooltipSection combat={combat} />}
      {ground && <GroundCombatTooltipSection ground={ground} />}
      {tech && <TechBenefitsTooltipSection tech={tech} />}
    </div>
  );
};
