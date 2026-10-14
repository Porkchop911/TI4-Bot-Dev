import React from "react";
import { TilePresentation } from "../../presentation/boardPresentation.ts";
import { computeTileEconomy, OverlayTileEconomy } from "../../presentation/mapOverlays.ts";
import { PlanetValue, PlanetValueGlyph, valueLabel } from "../PlanetValueIcons.tsx";

/** One economy card on the map: glyph + ready (/total) number, with the full word for screen readers. */
const EconomyCard: React.FC<{
  kind: "resources" | "influence";
  x: number;
  y: number;
  ready: number;
  total: number;
  exhausted: number;
}> = ({ kind, x, y, ready, total, exhausted }) => {
  const resource = kind === "resources";
  const text = `${ready}${exhausted > 0 ? `/${total}` : ""}`;
  const label =
    exhausted > 0
      ? `${valueLabel(kind, ready)} ready of ${total} total`
      : valueLabel(kind, ready);
  return (
    <g role="img" aria-label={label} data-testid={`economy-${kind}`}>
      <title>{label}</title>
      <rect
        x={x}
        y={y - 18}
        width={64}
        height={36}
        rx={6}
        fill={resource ? "#854d0e" : "#0369a1"}
        stroke={resource ? "#facc15" : "#38bdf8"}
        strokeWidth={2}
      />
      <PlanetValueGlyph kind={kind} x={x + 6} y={y - 8} size={16} />
      <text
        x={x + 26}
        y={y + 7}
        textAnchor="start"
        fill={resource ? "#fef08a" : "#bae6fd"}
        fontSize={text.length > 3 ? 14 : 19}
        fontWeight="900"
        aria-hidden="true"
      >
        {text}
      </text>
    </g>
  );
};

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
      <EconomyCard
        kind="resources"
        x={tile.center.x - 66}
        y={tile.center.y}
        ready={ecoOverlay.readyResources}
        total={ecoOverlay.totalResources}
        exhausted={ecoOverlay.exhaustedResources}
      />
      <EconomyCard
        kind="influence"
        x={tile.center.x + 2}
        y={tile.center.y}
        ready={ecoOverlay.readyInfluence}
        total={ecoOverlay.totalInfluence}
        exhausted={ecoOverlay.exhaustedInfluence}
      />

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

const EconomyLine: React.FC<{ title: string; r: number; i: number; color?: string }> = ({
  title,
  r,
  i,
  color,
}) => (
  <div
    style={{ color, display: "flex", gap: 8, alignItems: "center" }}
    aria-label={`${title}: ${valueLabel("resources", r)}, ${valueLabel("influence", i)}`}
    role="group"
  >
    <span>{title}:</span>
    <PlanetValue kind="resources" value={r} size="tooltip" />
    <PlanetValue kind="influence" value={i} size="tooltip" />
  </div>
);

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
      <EconomyLine title="Ready" r={eco.readyResources} i={eco.readyInfluence} />
      <EconomyLine title="Total" r={eco.totalResources} i={eco.totalInfluence} />
      {eco.exhaustedResources > 0 && (
        <EconomyLine title="Exhausted" r={eco.exhaustedResources} i={eco.exhaustedInfluence} color="#ef4444" />
      )}
    </div>
  );
};
