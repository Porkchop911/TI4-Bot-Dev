import React from "react";
import { TilePresentation } from "../../presentation/boardPresentation.ts";
import {
  computeTileTechBenefits,
  getTechSpecialtyStyle,
  OverlayTileTechBenefits,
} from "../../presentation/mapOverlays.ts";
import { SvgButton } from "../../primitives/index.ts";

export interface TechBenefitsOverlayProps {
  tile: TilePresentation;
  onSelectTarget?: (systemId: string, planetId?: string) => void;
  onSelectSystem?: (systemId: string | null) => void;
}

export const TechBenefitsOverlay: React.FC<TechBenefitsOverlayProps> = ({
  tile,
  onSelectTarget,
  onSelectSystem,
}) => {
  const techOverlay = computeTileTechBenefits(tile);
  const pCount = tile.planets.length;

  return (
    <>
      {/* Tech Benefits: Dim non-tech systems */}
      {!techOverlay?.hasTechSpecialties && (
        <polygon points={tile.points} fill="rgba(15, 23, 42, 0.55)" pointerEvents="none" />
      )}

      {/* Tech Benefits: Active system highlight border */}
      {techOverlay?.hasTechSpecialties && (
        <polygon
          points={tile.innerPoints}
          fill="none"
          stroke="#38bdf8"
          strokeWidth={2}
          strokeDasharray="4 2"
          pointerEvents="none"
        />
      )}

      {/* Tech Planets */}
      {tile.planets.map((p, pIdx) => {
        let pX = tile.center.x;
        let pY = tile.center.y;
        const planetRadius = 15;

        if (pCount === 1) {
          pX = tile.center.x;
          pY = tile.center.y + 2;
        } else if (pCount === 2) {
          pX = tile.center.x + (pIdx === 0 ? -30 : 30);
          pY = tile.center.y + (pIdx === 0 ? -12 : 16);
        } else if (pCount >= 3) {
          if (pIdx === 0) {
            pX = tile.center.x;
            pY = tile.center.y - 24;
          } else if (pIdx === 1) {
            pX = tile.center.x - 34;
            pY = tile.center.y + 16;
          } else {
            pX = tile.center.x + 34;
            pY = tile.center.y + 16;
          }
        }

        const isControlled = Boolean(p.controlledBy);
        const isCandidateTarget = Boolean(p.isCandidateTarget);
        const hasTech = Boolean(p.techSpecialties && p.techSpecialties.length > 0);

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
              opacity: !hasTech ? 0.25 : 1,
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

            {/* Tech Specialty Badge */}
            {hasTech &&
              (() => {
                const spec = p.techSpecialties![0];
                const specStyle = getTechSpecialtyStyle(spec);
                const badgeWidth = 92;
                const badgeHeight = 21;
                return (
                  <g data-testid={`tech-benefit-planet-${p.id}`} pointerEvents="none">
                    <circle
                      cx={pX}
                      cy={pY}
                      r={21}
                      fill="none"
                      stroke={specStyle.border}
                      strokeWidth={3.5}
                    />
                    <rect
                      x={pX - badgeWidth / 2}
                      y={pY - 29}
                      width={badgeWidth}
                      height={badgeHeight}
                      rx={5}
                      fill={specStyle.bg}
                      stroke={specStyle.border}
                      strokeWidth={1.8}
                    />
                    <text
                      x={pX}
                      y={pY - 14.5}
                      textAnchor="middle"
                      fill="#ffffff"
                      fontSize="11.5"
                      fontWeight="900"
                    >
                      {specStyle.symbol} {specStyle.label}
                    </text>
                    {p.exhausted && (
                      <line
                        x1={pX - 11}
                        y1={pY - 11}
                        x2={pX + 11}
                        y2={pY + 11}
                        stroke="#ef4444"
                        strokeWidth={3.5}
                      />
                    )}
                  </g>
                );
              })()}
          </SvgButton>
        );
      })}
    </>
  );
};

export interface TechBenefitsTooltipSectionProps {
  tech: OverlayTileTechBenefits;
}

export const TechBenefitsTooltipSection: React.FC<TechBenefitsTooltipSectionProps> = ({ tech }) => {
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
      <div style={{ fontWeight: "bold", color: "#c084fc", marginBottom: 2 }}>
        🔬 Tech Benefits Overlay
      </div>
      {tech.planets.length > 0 ? (
        tech.planets.map((tp) => (
          <div key={tp.id} style={{ marginTop: 2 }}>
            • {tp.label}: {tp.specialties.join(", ")} {tp.exhausted ? "(Exhausted)" : "(Ready)"}
          </div>
        ))
      ) : (
        <div style={{ color: "#94a3b8" }}>No tech specialty planets</div>
      )}
    </div>
  );
};
