import React from "react";
import { TilePresentation } from "../../presentation/boardPresentation.ts";
import { computeTileEconomy, OverlayTileEconomy } from "../../presentation/mapOverlays.ts";

export interface EconomyOverlayProps {
  tile: TilePresentation;
}

export const EconomyOverlay: React.FC<EconomyOverlayProps> = ({ tile }) => {
  const ecoOverlay = computeTileEconomy(tile);
  if (!ecoOverlay || !ecoOverlay.hasPlanets) {
    return null;
  }

  return (
    <g data-testid={`economy-overlay-${tile.systemId}`} pointerEvents="none">
      {/* Resource Card */}
      <rect
        x={tile.center.x - 58}
        y={tile.center.y - 18}
        width={56}
        height={36}
        rx={6}
        fill="#854d0e"
        stroke="#facc15"
        strokeWidth={2}
      />
      <text
        x={tile.center.x - 30}
        y={tile.center.y + 7}
        textAnchor="middle"
        fill="#fef08a"
        fontSize="19"
        fontWeight="900"
      >
        {ecoOverlay.readyResources}
        {ecoOverlay.exhaustedResources > 0 ? `/${ecoOverlay.totalResources}` : ""}
      </text>

      {/* Influence Card */}
      <rect
        x={tile.center.x + 2}
        y={tile.center.y - 18}
        width={56}
        height={36}
        rx={6}
        fill="#0369a1"
        stroke="#38bdf8"
        strokeWidth={2}
      />
      <text
        x={tile.center.x + 30}
        y={tile.center.y + 7}
        textAnchor="middle"
        fill="#bae6fd"
        fontSize="19"
        fontWeight="900"
      >
        {ecoOverlay.readyInfluence}
        {ecoOverlay.exhaustedInfluence > 0 ? `/${ecoOverlay.totalInfluence}` : ""}
      </text>

      {/* Sub-label if any planets are exhausted */}
      {(ecoOverlay.exhaustedResources > 0 || ecoOverlay.exhaustedInfluence > 0) && (
        <text
          x={tile.center.x}
          y={tile.center.y + 28}
          textAnchor="middle"
          fill="#fca5a5"
          fontSize="9.5"
          fontWeight="bold"
        >
          ⚠️ Exhausted
        </text>
      )}
    </g>
  );
};

export interface EconomyTooltipSectionProps {
  eco: OverlayTileEconomy;
}

export const EconomyTooltipSection: React.FC<EconomyTooltipSectionProps> = ({ eco }) => {
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
      <div style={{ fontWeight: "bold", color: "#fef08a", marginBottom: 2 }}>
        💰 Economy Overlay
      </div>
      <div>
        Ready: {eco.readyResources} Res / {eco.readyInfluence} Inf
      </div>
      <div>
        Total: {eco.totalResources} Res / {eco.totalInfluence} Inf
      </div>
      {eco.exhaustedResources > 0 && (
        <div style={{ color: "#ef4444" }}>
          Exhausted: {eco.exhaustedResources} Res / {eco.exhaustedInfluence} Inf
        </div>
      )}
    </div>
  );
};
