import React from "react";
import { TilePresentation } from "../../presentation/boardPresentation.ts";
import { computeTileSpaceCombat, OverlayTileSpaceCombat } from "../../presentation/mapOverlays.ts";
import { usePlayerIdentity } from "../../presentation/PlayerIdentity.tsx";
import { PlayerView } from "../../protocol/types.ts";

export function getSpaceCombatHexStyle(
  tile: TilePresentation,
  isSelected: boolean,
  combatOverlay: OverlayTileSpaceCombat | null,
) {
  const hasCombatUnits = Boolean(combatOverlay?.hasCombatUnits);
  const fleets = combatOverlay?.fleets ?? [];

  const stroke = isSelected
    ? "#facc15"
    : tile.isCandidateTarget
      ? "#38bdf8"
      : hasCombatUnits
        ? fleets.length > 1
          ? "#ef4444"
          : fleets[0].ownerColor
        : tile.strokeColor;

  const strokeWidth = isSelected
    ? 4
    : tile.isCandidateTarget
      ? 3.5
      : hasCombatUnits
        ? 3.5
        : tile.strokeWidth;

  const strokeDasharray = combatOverlay && fleets.length > 1 ? "6 3" : tile.strokeDashArray;

  return { stroke, strokeWidth, strokeDasharray };
}

export interface SpaceCombatOverlayProps {
  tile: TilePresentation;
  players?: readonly PlayerView[];
}

export const SpaceCombatOverlay: React.FC<SpaceCombatOverlayProps> = ({ tile, players }) => {
  const display = usePlayerIdentity();
  const combatOverlay = computeTileSpaceCombat(tile, players);

  const hasHighThreat = combatOverlay?.fleets.some((f) => f.avgHits >= 1.5);
  const hasCombatUnits = Boolean(combatOverlay?.hasCombatUnits);

  return (
    <>
      {/* High threat border */}
      {hasHighThreat && (
        <polygon
          points={tile.innerPoints}
          fill="none"
          stroke="#ef4444"
          strokeWidth={2}
          strokeDasharray="4 2"
          pointerEvents="none"
        />
      )}

      {/* Fleet Stats (Vertical borderless stats with controlling player symbol, planets dropped) */}
      {hasCombatUnits && combatOverlay && (
        <g data-testid={`space-combat-overlay-${tile.systemId}`} pointerEvents="none">
          {combatOverlay.fleets.length > 1 ? (
            /* Two or more players coexist in one hex: big fight symbol (swords) */
            <g data-testid={`space-combat-battle-${tile.systemId}`}>
              <text
                x={tile.center.x}
                y={tile.center.y + 16}
                textAnchor="middle"
                fontSize="52"
                filter="drop-shadow(0 0 14px rgba(239, 68, 68, 0.95))"
              >
                ⚔️
              </text>
              <text
                x={tile.center.x}
                y={tile.center.y + 40}
                textAnchor="middle"
                fill="#ef4444"
                stroke="#0b1220"
                strokeWidth={2.5}
                paintOrder="stroke"
                fontSize="11"
                fontWeight="900"
                letterSpacing="1.5"
              >
                BATTLE
              </text>
            </g>
          ) : (
            /* Single fleet in control: large stats with controlling player symbol */
            (() => {
              const fleet = combatOverlay.fleets[0];
              const hasSustain = fleet.sustainCount > 0;
              const playerInfo = display(fleet.owner);

              return (
                <g data-testid={`space-combat-fleet-${tile.systemId}-${fleet.owner}`}>
                  {/* Controlling Player Symbol */}
                  <text
                    x={tile.center.x - 50}
                    y={tile.center.y - 50}
                    textAnchor="middle"
                    fill={fleet.ownerColor}
                    stroke="#0b1220"
                    strokeWidth={3}
                    paintOrder="stroke"
                    fontSize="22"
                    fontWeight="900"
                  >
                    {playerInfo.symbol}
                  </text>

                  {/* Row 1: Ships on the left of number, with shields for sustain on the same line */}
                  <text
                    x={tile.center.x}
                    y={tile.center.y - 6}
                    textAnchor="middle"
                    fill="#f8fafc"
                    stroke="#0b1220"
                    strokeWidth={3}
                    paintOrder="stroke"
                    fontSize="22"
                    fontWeight="900"
                  >
                    🚀{fleet.totalUnits}
                    {hasSustain && (
                      <tspan fill="#38bdf8" dx="10">
                        🛡️{fleet.sustainCount}
                      </tspan>
                    )}
                  </text>

                  {/* Row 2: Average Hits per Round */}
                  <text
                    x={tile.center.x}
                    y={tile.center.y + 30}
                    textAnchor="middle"
                    fill="#fca5a5"
                    stroke="#0b1220"
                    strokeWidth={3.5}
                    paintOrder="stroke"
                    fontSize="22"
                    fontWeight="900"
                  >
                    ⚔️{fleet.avgHits.toFixed(1)}
                  </text>
                </g>
              );
            })()
          )}
        </g>
      )}
    </>
  );
};

export interface SpaceCombatTooltipSectionProps {
  combat: OverlayTileSpaceCombat;
}

export const SpaceCombatTooltipSection: React.FC<SpaceCombatTooltipSectionProps> = ({ combat }) => {
  const display = usePlayerIdentity();

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
      <div style={{ fontWeight: "bold", color: "#38bdf8", marginBottom: 2 }}>
        🚀 Space Combat Overlay
      </div>
      {combat.fleets.length > 0 ? (
        combat.fleets.map((f) => (
          <div key={f.owner} style={{ color: f.ownerColor, marginTop: 2 }}>
            • {display(f.owner).label}: {f.totalUnits} ships ({f.fighterCount} fighters) — ~
            {f.avgHits.toFixed(1)} hits/rd
            {f.sustainCount > 0 ? ` (${f.sustainCount} sustain)` : ""}
          </div>
        ))
      ) : (
        <div style={{ color: "#94a3b8" }}>No ships in space</div>
      )}
    </div>
  );
};
