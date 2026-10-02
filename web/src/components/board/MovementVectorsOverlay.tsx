import React from "react";
import { MovementVector } from "../../presentation/boardPresentation.ts";

export interface MovementVectorsOverlayProps {
  vectors: readonly MovementVector[];
}

export const MovementVectorsOverlay: React.FC<MovementVectorsOverlayProps> = ({ vectors }) => {
  return (
    <>
      {vectors.map((vec) => (
        <g key={`vec-${vec.fromSystemId}-${vec.toSystemId}`}>
          <line
            data-testid="movement-vector-line"
            x1={vec.fromCenter.x}
            y1={vec.fromCenter.y}
            x2={vec.toCenter.x}
            y2={vec.toCenter.y}
            stroke="#4ade80"
            strokeWidth={3}
            strokeDasharray="8 5"
            markerEnd="url(#vector-arrow)"
            style={{ opacity: 0.85, pointerEvents: "none" }}
          />
          <circle
            cx={(vec.fromCenter.x + vec.toCenter.x) / 2}
            cy={(vec.fromCenter.y + vec.toCenter.y) / 2}
            r={10}
            fill="#0f172a"
            stroke="#4ade80"
            strokeWidth={1.5}
            style={{ pointerEvents: "none" }}
          />
          <text
            x={(vec.fromCenter.x + vec.toCenter.x) / 2}
            y={(vec.fromCenter.y + vec.toCenter.y) / 2 + 4}
            textAnchor="middle"
            fill="#4ade80"
            fontSize="10"
            fontWeight="bold"
            style={{ pointerEvents: "none" }}
          >
            {vec.unitCount}
          </text>
        </g>
      ))}
    </>
  );
};
