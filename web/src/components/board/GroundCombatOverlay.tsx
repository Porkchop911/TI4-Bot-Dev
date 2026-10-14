import React from "react";
import { TilePresentation } from "../../presentation/boardPresentation.ts";
import {
  computeTileGroundCombat,
  OverlayTileGroundCombat,
} from "../../presentation/mapOverlays.ts";
import { SvgButton } from "../../primitives/index.ts";
import { PlayerView } from "../../protocol/types.ts";

export interface GroundCombatOverlayProps {
  tile: TilePresentation;
  players?: readonly PlayerView[];
  onSelectTarget?: (systemId: string, planetId?: string) => void;
  onSelectSystem?: (systemId: string | null) => void;
}

export const GroundCombatOverlay: React.FC<GroundCombatOverlayProps> = ({
  tile,
  players,
  onSelectTarget,
  onSelectSystem,
}) => {
  const groundOverlay = computeTileGroundCombat(tile, players);
  const pCount = tile.planets.length;

  return (
    <>
      {tile.planets.map((p, pIdx) => {
        let pX = tile.center.x;
        let pY = tile.center.y;
        let planetRadius = 26;

        if (pCount === 1) {
          planetRadius = 26;
          pX = tile.center.x;
          pY = tile.center.y;
        } else if (pCount === 2) {
          planetRadius = 24;
          pX = tile.center.x + (pIdx === 0 ? -32 : 32);
          pY = tile.center.y + (pIdx === 0 ? -12 : 16);
        } else if (pCount >= 3) {
          planetRadius = 21;
          if (pIdx === 0) {
            pX = tile.center.x;
            pY = tile.center.y - 25;
          } else if (pIdx === 1) {
            pX = tile.center.x - 35;
            pY = tile.center.y + 20;
          } else {
            pX = tile.center.x + 35;
            pY = tile.center.y + 20;
          }
        }

        const isControlled = Boolean(p.controlledBy);
        const isCandidateTarget = Boolean(p.isCandidateTarget);
        const isLarge = planetRadius >= 24;
        const pSummary = groundOverlay?.planets.find((gp) => gp.id === p.id);

        return (
          <SvgButton
            key={p.id}
            data-testid={`planet-${p.id}`}
            data-target-candidate={isCandidateTarget ? "true" : undefined}
            isInteractive={isCandidateTarget}
            label={`Target planet ${p.label}`}
            onActivate={() => {
              if (isCandidateTarget) {
                // Inspect first: a system inspection may reset the selection, the target must win.
                onSelectSystem?.(tile.systemId);
                onSelectTarget?.(tile.systemId, p.id);
              }
            }}
            onKeyDown={(e) => {
              if (isCandidateTarget && (e.key === "Enter" || e.key === " ")) {
                e.stopPropagation();
              }
            }}
            onClick={(e) => {
              if (isCandidateTarget) {
                e.stopPropagation();
                // Inspect first: a system inspection may reset the selection, the target must win.
                onSelectSystem?.(tile.systemId);
                onSelectTarget?.(tile.systemId, p.id);
              }
            }}
            style={{
              cursor: isCandidateTarget ? "pointer" : "inherit",
            }}
          >
            <circle
              cx={pX}
              cy={pY}
              r={planetRadius}
              fill="#090d16"
              stroke={isCandidateTarget ? "#38bdf8" : isControlled ? p.controllerColor : "#64748b"}
              strokeWidth={3.5}
            />

            {pSummary && pSummary.hasForces ? (
              <g data-testid={`ground-combat-overlay-${p.id}`} pointerEvents="none">
                {/* Defenders Count inside circle */}
                <text
                  x={pX}
                  y={pY - 1}
                  textAnchor="middle"
                  fill="#ffffff"
                  stroke="#000000"
                  strokeWidth={2}
                  paintOrder="stroke"
                  fontSize={isLarge ? "18" : "16"}
                  fontWeight="900"
                >
                  {pSummary.totalDefenders}
                </text>

                {/* Average Hits inside circle */}
                <text
                  x={pX}
                  y={pY + (isLarge ? 15 : 13)}
                  textAnchor="middle"
                  fill="#fca5a5"
                  stroke="#000000"
                  strokeWidth={2}
                  paintOrder="stroke"
                  fontSize={isLarge ? "15" : "13.5"}
                  fontWeight="900"
                >
                  {pSummary.forces[0]?.avgHits.toFixed(1) ?? "0.0"}
                </text>

                {/* Planetary Shield indicator on top-right rim */}
                {pSummary.forces[0]?.hasPlanetaryShield && (
                  <text
                    x={pX + planetRadius - 2}
                    y={pY - planetRadius + 6}
                    textAnchor="middle"
                    fontSize={isLarge ? "15" : "13"}
                    filter="drop-shadow(0 0 4px #0284c7)"
                  >
                    🛡️
                  </text>
                )}
              </g>
            ) : (
              <g data-testid={`ground-combat-overlay-${p.id}`} pointerEvents="none">
                <text
                  x={pX}
                  y={pY + 6}
                  textAnchor="middle"
                  fill="#94a3b8"
                  stroke="#000000"
                  strokeWidth={2}
                  paintOrder="stroke"
                  fontSize={isLarge ? "18" : "16"}
                  fontWeight="bold"
                >
                  0
                </text>
              </g>
            )}
          </SvgButton>
        );
      })}
    </>
  );
};

export interface GroundCombatTooltipSectionProps {
  ground: OverlayTileGroundCombat;
}

export const GroundCombatTooltipSection: React.FC<GroundCombatTooltipSectionProps> = ({
  ground,
}) => {
  return (
    <div
      data-testid="system-tooltip-overlay"
      style={{
        marginTop: 6,
        paddingTop: 6,
        borderTop: "1px solid rgba(255,255,255,0.15)",
        fontSize: 12,
      }}
    >
      <div style={{ fontWeight: "bold", color: "#4ade80", marginBottom: 2 }}>
        🪖 Ground Combat Overlay
      </div>
      {ground.planets.length > 0 ? (
        ground.planets.map((gp) => (
          <div key={gp.id} style={{ marginTop: 2 }}>
            • {gp.label}: {gp.totalDefenders} defenders — ~
            {gp.forces[0]?.avgHits.toFixed(1) ?? "0.0"} hits/rd
            {gp.forces[0]?.hasPlanetaryShield ? " [🛡️ Shield]" : ""}
            {gp.forces[0]?.spaceCannonDefenseHits
              ? ` [🎯 PDS ${gp.forces[0].spaceCannonDefenseHits} hits]`
              : ""}
          </div>
        ))
      ) : (
        <div style={{ color: "#94a3b8" }}>No planets</div>
      )}
    </div>
  );
};
