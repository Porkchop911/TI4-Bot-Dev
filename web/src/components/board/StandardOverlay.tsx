import React from "react";
import { TilePresentation } from "../../presentation/boardPresentation.ts";
import { SvgButton } from "../../primitives/index.ts";
import { usePlayerIdentity } from "../../presentation/PlayerIdentity.tsx";

export interface StandardOverlayProps {
  tile: TilePresentation;
  onSelectTarget?: (systemId: string, planetId?: string) => void;
  onSelectSystem?: (systemId: string | null) => void;
}

export const StandardOverlay: React.FC<StandardOverlayProps> = ({
  tile,
  onSelectTarget,
  onSelectSystem,
}) => {
  const display = usePlayerIdentity();
  const pCount = tile.planets.length;

  return (
    <>
      {/* Planets */}
      {tile.planets.map((p, pIdx) => {
        const pX = tile.center.x + (pCount === 1 ? 0 : (pIdx - (pCount - 1) / 2) * 38);
        const pY = tile.center.y + 16;
        const planetRadius = 15;
        const isControlled = Boolean(p.controlledBy);
        const isCandidateTarget = Boolean(p.isCandidateTarget);

        return (
          <SvgButton
            key={p.id}
            data-testid={`planet-${p.id}`}
            data-target-candidate={isCandidateTarget ? "true" : undefined}
            isInteractive={isCandidateTarget}
            label={`Target planet ${p.label}`}
            onActivate={() => {
              if (isCandidateTarget) {
                onSelectTarget?.(tile.systemId, p.id);
                onSelectSystem?.(tile.systemId);
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
                onSelectTarget?.(tile.systemId, p.id);
                onSelectSystem?.(tile.systemId);
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
              fill={isControlled ? p.controllerColor : "#1e293b"}
              stroke={
                isCandidateTarget
                  ? "#38bdf8"
                  : p.exhausted
                    ? "#ef4444"
                    : isControlled
                      ? "#f8fafc"
                      : "#64748b"
              }
              strokeWidth={isCandidateTarget ? 3 : 2}
            />

            {p.controlledBy && (
              <text
                x={pX + 13}
                y={pY - 11}
                textAnchor="middle"
                fill="#fff"
                stroke="#0b1220"
                strokeWidth="0.6"
                paintOrder="stroke"
                fontSize="12"
                pointerEvents="none"
              >
                {display(p.controlledBy).symbol}
              </text>
            )}

            {/* Planet Abbreviation */}
            <text
              x={pX}
              y={pY - 2}
              textAnchor="middle"
              fill="#f8fafc"
              fontSize="8.5"
              fontWeight="bold"
              pointerEvents="none"
            >
              {p.id.substring(0, 3).toUpperCase()}
            </text>

            {/* Resources / Influence fraction */}
            {p.resources !== undefined && p.influence !== undefined && (
              <text
                x={pX}
                y={pY + 8}
                textAnchor="middle"
                fill="#fef08a"
                fontSize="7.5"
                fontWeight="bold"
                pointerEvents="none"
              >
                {p.resources}/{p.influence}
              </text>
            )}
          </SvgButton>
        );
      })}

      {/* Units Badge: ONLY shown in standard view */}
      {tile.totalUnits > 0 && (
        <g>
          <rect
            x={tile.center.x - 26}
            y={tile.center.y + 36}
            width="52"
            height="16"
            rx="4"
            fill="rgba(15, 23, 42, 0.9)"
            stroke="#475569"
            strokeWidth="1"
          />
          <text
            x={tile.center.x}
            y={tile.center.y + 48}
            textAnchor="middle"
            fill="#e2e8f0"
            fontSize="9"
            fontWeight="bold"
            pointerEvents="none"
          >
            {tile.totalUnits} units
          </text>
        </g>
      )}
    </>
  );
};
