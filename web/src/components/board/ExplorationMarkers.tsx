import React from "react";
import type { TilePresentation } from "../../presentation/boardPresentation.ts";
import { attachmentLabels, explorationsInSystem } from "../../presentation/explorationMarks.ts";
import { usePublicState } from "../../presentation/PublicStateContext.tsx";
import { usePlayerIdentity } from "../../presentation/PlayerIdentity.tsx";

const RADIUS = 7;
/**
 * Where the markers go, as offsets from the tile centre: the upper-right of the hex, which the
 * planet discs (out to x 52 on a two-planet tile, down to y -45 on the top disc of a three-planet
 * tile), the "#id" label, the ACTIVATED badge, value pills, units badge and command tokens leave
 * free. At most three markers exist (frontier, attachments, explored).
 */
const SLOTS = [
  { dx: 22, dy: -42 },
  { dx: 38, dy: -42 },
  { dx: 46, dy: -28 },
];

/**
 * What exploring left on a system, drawn on its tile: a frontier token still waiting, the
 * attachments on its planets, and the explorations resolved here (count; the hover text names the
 * card and what was gained). Display only; none of it takes clicks.
 */
export const ExplorationMarkers: React.FC<{ tile: TilePresentation }> = ({ tile }) => {
  const { explorations, frontierSystems } = usePublicState();
  const display = usePlayerIdentity();
  const attachments = tile.planets.flatMap((planet) =>
    attachmentLabels(planet.attachments).map((label) => `${planet.label}: ${label}`),
  );
  const entries = explorationsInSystem(
    explorations,
    tile.systemId,
    tile.planets.map((planet) => planet.id),
  );
  const marks: { key: string; glyph: string; fill: string; stroke: string; title: string }[] = [];
  if (frontierSystems.has(tile.systemId)) {
    marks.push({
      key: "frontier",
      glyph: "?",
      fill: "#451a03",
      stroke: "#f59e0b",
      title: "Frontier token: ships here can explore it",
    });
  }
  if (attachments.length > 0) {
    marks.push({
      key: "attachments",
      glyph: attachments.length > 1 ? String(attachments.length) : "+",
      fill: "#2e1065",
      stroke: "#a78bfa",
      title: `Attachments\n${attachments.join("\n")}`,
    });
  }
  if (entries.length > 0) {
    marks.push({
      key: "explored",
      glyph: String(entries.length),
      fill: "#042f2e",
      stroke: "#2dd4bf",
      title: `Explored here\n${entries
        .map(
          (e) =>
            `${e.round ? `Round ${e.round}, ` : ""}${display(e.player).label}: ${e.card} (${e.where}) - ${e.result}`,
        )
        .join("\n")}`,
    });
  }
  if (marks.length === 0) return null;
  return (
    <g data-testid={`exploration-markers-${tile.systemId}`}>
      {marks.map((mark, index) => {
        const slot = SLOTS[index];
        const cx = tile.center.x + slot.dx;
        const cy = tile.center.y + slot.dy;
        return (
          <g key={mark.key} data-testid={`exploration-mark-${mark.key}-${tile.systemId}`}>
            <title>{mark.title}</title>
            <circle cx={cx} cy={cy} r={RADIUS} fill={mark.fill} stroke={mark.stroke} strokeWidth="1.5" />
            <text
              x={cx}
              y={cy + 3.5}
              textAnchor="middle"
              fill={mark.stroke}
              fontSize="10"
              fontWeight="bold"
              pointerEvents="none"
            >
              {mark.glyph}
            </text>
          </g>
        );
      })}
    </g>
  );
};
