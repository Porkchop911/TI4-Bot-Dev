import { boostsFor, type PlanetBoost } from "@ti4/shared/planets/values.ts";
import { boostLines } from "../../presentation/planetBoostText.ts";
import React from "react";
import {
  CONTROL_SYMBOL_FONT_SIZE,
  PLANET_VALUE_FONT_SIZE,
  PLANET_VALUE_PILL_HEIGHT,
  TILE_HALF_WIDTH,
  UNITS_BADGE,
  planetSlots,
} from "./boardLayout.ts";
import { MapTargetMode, TilePresentation } from "../../presentation/boardPresentation.ts";
import { SvgButton } from "../../primitives/index.ts";
import type { PaymentMark } from "../../presentation/paymentDraft.ts";
import { usePlayerIdentity } from "../../presentation/PlayerIdentity.tsx";
import { PlanetValueGlyph, valueKind, valueLabel } from "../PlanetValueIcons.tsx";

export interface StandardOverlayProps {
  tile: TilePresentation;
  onSelectTarget?: (systemId: string, planetId?: string) => void;
  onSelectSystem?: (systemId: string | null) => void;
  /** In planet mode candidates get a reticle and every other planet is dimmed. */
  targetMode?: MapTargetMode;
  /** While paying: worth and staged state per payable planet id. */
  paymentMarks?: ReadonlyMap<string, PaymentMark>;
}


const GLYPH = 10;
/** Number colour of a planet value raised by an attachment or law (matches --value-boost). */
const BOOSTED_FILL = "#4ade80";
const CHAR_W = PLANET_VALUE_FONT_SIZE * 0.6;

/** "3 resources, 1 influence" plus "Mining World: +2 resources" for each booster, for the pill's tooltip. */
function planetPillLabel(p: { resources: number; influence: number; boosts?: readonly PlanetBoost[] }): string {
  const lines = [...boostLines(p.boosts, "resources"), ...boostLines(p.boosts, "influence")];
  const base = `${valueLabel("resources", p.resources)}, ${valueLabel("influence", p.influence)}`;
  return lines.length ? `${base} (${lines.join("; ")})` : base;
}

/** Width of a value pill holding glyph + text per entry (null entries are skipped). */
function valuePillWidth(texts: (string | null)[]): number {
  const used = texts.filter((t): t is string => t !== null);
  return Math.ceil(8 + used.reduce((w, t) => w + GLYPH + 2 + t.length * CHAR_W + 4, -4));
}

/** Glyph + number pairs laid left to right inside a pill at (left, top). */
const ValuePillContent: React.FC<{
  left: number;
  top: number;
  entries: { kind: "resources" | "influence"; text: string; fill: string; boosted?: boolean }[];
}> = ({ left, top, entries }) => {
  let x = left + 4;
  return (
    <>
      {entries.map((e) => {
        const gx = x;
        x += GLYPH + 2 + e.text.length * CHAR_W + 4;
        return (
          <React.Fragment key={e.kind}>
            <PlanetValueGlyph kind={e.kind} x={gx} y={top + (PLANET_VALUE_PILL_HEIGHT - GLYPH) / 2} size={GLYPH} />
            <text
              x={gx + GLYPH + 2}
              y={top + PLANET_VALUE_PILL_HEIGHT / 2 + PLANET_VALUE_FONT_SIZE * 0.36}
              fill={e.boosted ? BOOSTED_FILL : e.fill}
              fontSize={PLANET_VALUE_FONT_SIZE}
              fontWeight="bold"
              aria-hidden="true"
            >
              {e.text}
            </text>
          </React.Fragment>
        );
      })}
    </>
  );
};

export const StandardOverlay: React.FC<StandardOverlayProps> = ({
  tile,
  onSelectTarget,
  onSelectSystem,
  targetMode = null,
  paymentMarks,
}) => {
  const display = usePlayerIdentity();
  const slots = planetSlots(tile.planets.length);

  return (
    <>
      {/* Planets */}
      {tile.planets.map((p, pIdx) => {
        const slot = slots[Math.min(pIdx, slots.length - 1)];
        const pX = tile.center.x + slot.dx;
        const pY = tile.center.y + slot.dy;
        const planetRadius = slot.r;
        // The value pill hangs on the disc's lower edge, kept inside the hex.
        const pillY = pY + planetRadius - PLANET_VALUE_PILL_HEIGHT / 2;
        const clampLeft = (left: number, width: number) =>
          Math.max(
            tile.center.x - TILE_HALF_WIDTH,
            Math.min(left, tile.center.x + TILE_HALF_WIDTH - width),
          );
        const isControlled = Boolean(p.controlledBy);
        const isCandidateTarget = Boolean(p.isCandidateTarget);
        const isPlanetMode = targetMode === "planet" || targetMode === "payment";
        const isDimmed = isPlanetMode && !isCandidateTarget;
        const mark =
          targetMode === "payment" || targetMode === "planet" ? paymentMarks?.get(p.id) : undefined;

        return (
          <SvgButton
            key={p.id}
            data-testid={`planet-${p.id}`}
            data-target-candidate={isCandidateTarget ? "true" : undefined}
            data-target-dimmed={isDimmed ? "true" : undefined}
            data-payment-staged={mark ? String(mark.staged) : undefined}
            isInteractive={isCandidateTarget}
            label={
              mark?.readies
                ? `${mark.staged ? "Do not ready" : "Ready"} planet ${p.label}`
                : mark
                  ? `${mark.staged ? "Stop exhausting" : "Exhaust"} planet ${p.label} for ${valueLabel(valueKind(mark.unit), mark.worth)}`
                  : `Target planet ${p.label}`
            }
            onActivate={() => {
              if (isCandidateTarget) {
                // Inspect first: a system inspection may reset the selection, the target must win.
                // Paying toggles planets one after another, so the inspector must not pop up.
                if (targetMode !== "payment" && !mark) onSelectSystem?.(tile.systemId);
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
                // Paying toggles planets one after another, so the inspector must not pop up.
                if (targetMode !== "payment" && !mark) onSelectSystem?.(tile.systemId);
                onSelectTarget?.(tile.systemId, p.id);
              }
            }}
            style={{
              cursor: isCandidateTarget ? "pointer" : isDimmed ? "not-allowed" : "inherit",
              opacity: isDimmed ? 0.35 : 1,
            }}
          >
            {isPlanetMode && isCandidateTarget && (
              <g
                data-testid={`planet-target-reticle-${p.id}`}
                pointerEvents="none"
                className="planet-target-reticle"
              >
                <circle cx={pX} cy={pY} r={planetRadius + 6} fill="rgba(56, 189, 248, 0.18)" />
                <circle
                  cx={pX}
                  cy={pY}
                  r={planetRadius + 6}
                  fill="none"
                  stroke={mark?.staged ? "#4ade80" : "#38bdf8"}
                  strokeWidth={mark?.staged ? 3 : 2}
                  strokeDasharray={mark?.staged ? undefined : "4 2"}
                  className={mark?.staged ? undefined : "target-pulse-ring"}
                />
                {[0, 90, 180, 270].map((angle) => {
                  const rad = (angle * Math.PI) / 180;
                  const inner = planetRadius + 7;
                  const outer = planetRadius + 11;
                  return (
                    <line
                      key={angle}
                      x1={pX + inner * Math.cos(rad)}
                      y1={pY + inner * Math.sin(rad)}
                      x2={pX + outer * Math.cos(rad)}
                      y2={pY + outer * Math.sin(rad)}
                      stroke={mark?.staged ? "#4ade80" : "#38bdf8"}
                      strokeWidth={2}
                    />
                  );
                })}
              </g>
            )}
            {isCandidateTarget && (
              // A larger invisible target than the drawn disc, for fingers.
              <circle cx={pX} cy={pY} r={planetRadius + 5} fill="transparent" />
            )}
            {p.legendary && (
              <circle
                data-testid={`planet-legendary-${p.id}`}
                cx={pX}
                cy={pY}
                r={planetRadius + 3}
                fill="none"
                stroke="#facc15"
                strokeWidth={2}
                pointerEvents="none"
              />
            )}
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
                x={pX + planetRadius * 0.9}
                y={pY - planetRadius * 0.7}
                textAnchor="middle"
                fill="#fff"
                stroke="#0b1220"
                strokeWidth="1"
                paintOrder="stroke"
                fontSize={CONTROL_SYMBOL_FONT_SIZE}
                pointerEvents="none"
              >
                {display(p.controlledBy).symbol}
              </text>
            )}

            {/* Planet abbreviation, in the disc */}
            <text
              x={pX}
              y={pY + (planetRadius >= 21 ? 4 : 3.5)}
              textAnchor="middle"
              fill="#f8fafc"
              stroke="#0b1220"
              strokeWidth="1.6"
              strokeLinejoin="round"
              paintOrder="stroke"
              letterSpacing="1"
              fontSize={planetRadius >= 21 ? "11.5" : "10"}
              fontWeight="bold"
              pointerEvents="none"
            >
              {p.id.substring(0, 3).toUpperCase()}
            </text>

            {p.legendary && (
              <text
                x={pX - planetRadius * 0.85}
                y={pY - planetRadius * 0.45}
                textAnchor="middle"
                fill="#facc15"
                stroke="#0b1220"
                strokeWidth="2.5"
                paintOrder="stroke"
                fontSize="14"
                pointerEvents="none"
                aria-hidden="true"
              >
                ★
              </text>
            )}

            {mark && isCandidateTarget
              ? (() => {
                  const lost = mark.resources ?? p.resources;
                  // Paying with resources: the planet's influence is shown beside it, and vice versa.
                  const otherKind = mark.unit === "I" ? "resources" : "influence";
                  const other = mark.unit === "I" ? lost : p.influence;
                  const wide = other !== undefined;
                  const label = `${mark.staged ? (mark.readies ? "To ready: " : "Staged: ") : ""}${valueLabel(valueKind(mark.unit), mark.worth)}${wide ? `${mark.readies ? ", " : mark.unit === "I" ? ", exhausting loses " : ", also "}${valueLabel(otherKind, other)}` : ""}`;
                  const first = `${mark.staged ? "✓ " : ""}${mark.worth}`;
                  const width = valuePillWidth([first, wide ? String(other) : null]);
                  const left = clampLeft(pX - width / 2, width);
                  return (
                    <g data-testid={`payment-mark-${p.id}`} pointerEvents="none" role="img" aria-label={label}>
                      <title>{label}</title>
                      <rect
                        x={left}
                        y={pillY}
                        width={width}
                        height={PLANET_VALUE_PILL_HEIGHT}
                        rx={PLANET_VALUE_PILL_HEIGHT / 2}
                        fill={mark.staged ? "#166534" : "#0c4a6e"}
                        stroke={mark.staged ? "#4ade80" : "#38bdf8"}
                        strokeWidth={1.5}
                      />
                      <ValuePillContent
                        left={left}
                        top={pillY}
                        entries={[
                          { kind: valueKind(mark.unit), text: first, fill: "#f8fafc" },
                          ...(wide ? [{ kind: otherKind as "resources" | "influence", text: String(other), fill: "#fef08a" }] : []),
                        ]}
                      />
                    </g>
                  );
                })()
              : p.resources !== undefined &&
                p.influence !== undefined && (
                  <g
                    pointerEvents="none"
                    role="img"
                    aria-label={planetPillLabel(p)}
                    data-boosted={p.boosts?.length ? "true" : undefined}
                  >
                    <title>{planetPillLabel(p)}</title>
                    {(() => {
                      const rBoost = boostsFor(p.boosts, "resources").length > 0;
                      const iBoost = boostsFor(p.boosts, "influence").length > 0;
                      const rText = `${p.resources}${rBoost ? "+" : ""}`;
                      const iText = `${p.influence}${iBoost ? "+" : ""}`;
                      const width = valuePillWidth([rText, iText]);
                      const left = clampLeft(pX - width / 2, width);
                      return (
                        <>
                          <rect
                            x={left}
                            y={pillY}
                            width={width}
                            height={PLANET_VALUE_PILL_HEIGHT}
                            rx={PLANET_VALUE_PILL_HEIGHT / 2}
                            fill="rgba(11, 18, 32, 0.94)"
                            stroke="#64748b"
                            strokeWidth={1}
                          />
                          <ValuePillContent
                            left={left}
                            top={pillY}
                            entries={[
                              { kind: "resources", text: rText, fill: "#fef08a", boosted: rBoost },
                              { kind: "influence", text: iText, fill: "#bae6fd", boosted: iBoost },
                            ]}
                          />
                        </>
                      );
                    })()}
                  </g>
                )}
          </SvgButton>
        );
      })}

      {/* Units Badge: ONLY shown in standard view */}
      {tile.totalUnits > 0 && (
        <g>
          <rect
            x={tile.center.x - UNITS_BADGE.width / 2}
            y={tile.center.y + UNITS_BADGE.top}
            width={UNITS_BADGE.width}
            height={UNITS_BADGE.height}
            rx="4"
            fill="rgba(15, 23, 42, 0.9)"
            stroke="#475569"
            strokeWidth="1"
          />
          <text
            x={tile.center.x}
            y={tile.center.y + UNITS_BADGE.top + UNITS_BADGE.height - 4}
            textAnchor="middle"
            fill="#e2e8f0"
            fontSize={UNITS_BADGE.fontSize}
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
