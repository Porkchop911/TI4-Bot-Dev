import React from "react";
import { MovementVector } from "../../presentation/boardPresentation.ts";

export interface MovementVectorsOverlayProps {
  vectors: readonly MovementVector[];
}

/** Arrow colours: the movement dialog's green, and red for other players' ships (who-can-go-here). */
export const VECTOR_COLORS = { own: "#4ade80", enemy: "#f87171" } as const;

export const MovementVectorsOverlay: React.FC<MovementVectorsOverlayProps> = ({ vectors }) => {
  return (
    <>
      {vectors.map((vec) => {
        const color = VECTOR_COLORS[vec.tone ?? "own"];
        const marker = vec.tone === "enemy" ? "vector-arrow-enemy" : "vector-arrow";
        return (
        <g
          key={`vec-${vec.tone ?? ""}-${vec.fromSystemId}-${vec.toSystemId}`}
          data-testid={vec.tone ? `reach-vector-${vec.tone}` : undefined}
          style={{ opacity: vec.dimmed ? 0.25 : 1 }}
        >
          <line
            data-testid="movement-vector-line"
            x1={vec.fromCenter.x}
            y1={vec.fromCenter.y}
            x2={vec.toCenter.x}
            y2={vec.toCenter.y}
            stroke={color}
            strokeWidth={3}
            strokeDasharray={vec.tone === "enemy" ? "3 6" : "8 5"}
            markerEnd={`url(#${marker})`}
            style={{ opacity: 0.85, pointerEvents: "none" }}
          />
          <circle
            cx={(vec.fromCenter.x + vec.toCenter.x) / 2}
            cy={(vec.fromCenter.y + vec.toCenter.y) / 2}
            r={10}
            fill="#0f172a"
            stroke={color}
            strokeWidth={1.5}
            style={{ pointerEvents: "none" }}
          />
          <text
            x={(vec.fromCenter.x + vec.toCenter.x) / 2}
            y={(vec.fromCenter.y + vec.toCenter.y) / 2 + 4}
            textAnchor="middle"
            fill={color}
            fontSize="10"
            fontWeight="bold"
            style={{ pointerEvents: "none" }}
          >
            {vec.unitCount}
          </text>
        </g>
        );
      })}
    </>
  );
};
