import React from "react";
import { MapTargetMode, TilePresentation } from "../../presentation/boardPresentation.ts";
import { MapOverlayMode, computeTileSpaceCombat } from "../../presentation/mapOverlays.ts";
import { SvgButton } from "../../primitives/index.ts";
import { usePlayerIdentity } from "../../presentation/PlayerIdentity.tsx";
import { seatInk } from "../../presentation/playerDisplay.ts";
import { EconomyOverlay } from "./EconomyOverlay.tsx";
import { SpaceCombatOverlay, getSpaceCombatHexStyle } from "./SpaceCombatOverlay.tsx";
import { GroundCombatOverlay } from "./GroundCombatOverlay.tsx";
import { TechBenefitsOverlay } from "./TechBenefitsOverlay.tsx";
import { StandardOverlay } from "./StandardOverlay.tsx";
import { ExplorationMarkers } from "./ExplorationMarkers.tsx";
import { AnomalyArt, AnomalyBadge } from "../anomaly/AnomalyArt.tsx";
import { COMMAND_TOKEN_RADIUS, COMMAND_TOKEN_STEP, COMMAND_TOKEN_Y } from "./boardLayout.ts";

import { PlayerView } from "../../protocol/types.ts";
import type { PaymentMark } from "../../presentation/paymentDraft.ts";

export interface BoardTileProps {
  tile: TilePresentation;
  isSelected: boolean;
  activeOverlay: MapOverlayMode;
  viewerSeat: string | null;
  isActivationMode: boolean;
  /** Defaults to "system" when `isActivationMode` is set. */
  targetMode?: MapTargetMode;
  players?: readonly PlayerView[];
  /** While paying: what each payable planet is worth and whether it is staged. */
  paymentMarks?: ReadonlyMap<string, PaymentMark>;
  onSelectTarget?: (systemId: string, planetId?: string) => void;
  onSelectOptionId?: (optionId: string) => void;
  onSelectSystem?: (systemId: string | null) => void;
  onMouseEnter: () => void;
  onMouseLeave: () => void;
}

export const BoardTile: React.FC<BoardTileProps> = ({
  tile,
  isSelected,
  activeOverlay,
  viewerSeat,
  isActivationMode,
  targetMode: targetModeProp,
  players,
  paymentMarks,
  onSelectTarget,
  onSelectOptionId,
  onSelectSystem,
  onMouseEnter,
  onMouseLeave,
}) => {
  const display = usePlayerIdentity();
  const sysId = tile.systemId;

  const combatOverlay =
    activeOverlay === "space_combat" ? computeTileSpaceCombat(tile, players) : null;

  const hexStyle =
    activeOverlay === "space_combat"
      ? getSpaceCombatHexStyle(tile, isSelected, combatOverlay)
      : {
          stroke: isSelected ? "#facc15" : tile.isCandidateTarget ? "#38bdf8" : tile.strokeColor,
          strokeWidth: isSelected ? 4 : tile.isCandidateTarget ? 3.5 : tile.strokeWidth,
          strokeDasharray: tile.strokeDashArray,
        };

  const targetMode = targetModeProp ?? (isActivationMode ? "system" : null);

  const handleTileClick = () => {
    if (targetMode === "payment") {
      // Planets are toggled on their own token; the hex only inspects.
      onSelectSystem?.(tile.systemId);
      return;
    }
    if (targetMode === "planet") {
      // The hex is never an answer in planet mode: inspect it, and pick its planet only when it is
      // the system's single candidate (otherwise the choice would be ambiguous).
      onSelectSystem?.(tile.systemId);
      if (tile.singleCandidatePlanetId)
        onSelectTarget?.(tile.systemId, tile.singleCandidatePlanetId);
      return;
    }
    onSelectTarget?.(tile.systemId);
    if (!tile.isCandidateTarget) {
      onSelectOptionId?.("");
    }
    onSelectSystem?.(tile.systemId);
  };

  return (
    <SvgButton
      key={`hex-${sysId}`}
      data-testid={`system-hex-${sysId}`}
      data-system-id={sysId}
      data-target-candidate={tile.isCandidateTarget ? "true" : undefined}
      data-context-subject={tile.isContextSubject ? "true" : undefined}
      data-system-selected={isSelected ? "true" : undefined}
      isInteractive
      data-single-candidate-planet={tile.singleCandidatePlanetId ?? undefined}
      label={`${tile.isCandidateTarget ? "Target" : "Inspect"} system ${tile.label} #${sysId}`}
      onActivate={handleTileClick}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      style={{
        cursor: "pointer",
        outline: "none",
      }}
    >
      {/* Hexagon Tile */}
      <polygon
        points={tile.points}
        fill={tile.fillColor}
        stroke={hexStyle.stroke}
        strokeWidth={hexStyle.strokeWidth}
        strokeDasharray={hexStyle.strokeDasharray}
        filter={tile.isCandidateTarget ? "url(#target-glow)" : undefined}
      />

      {/* Anomaly art: outer-ring texture behind everything else */}
      <AnomalyArt anomalies={tile.anomalies} cx={tile.center.x} cy={tile.center.y} />

      {/* Candidate target soft glow fill */}
      {tile.isCandidateTarget && isActivationMode && (
        <polygon points={tile.points} fill="rgba(56, 189, 248, 0.15)" pointerEvents="none" />
      )}

      {/* Candidate target highlight animation ring */}
      {tile.isCandidateTarget && (
        <polygon
          points={tile.innerPoints}
          fill="none"
          stroke="#38bdf8"
          strokeWidth={2}
          strokeDasharray="5 3"
          className="target-pulse-ring"
        />
      )}

      {/* Activation Mode Target Reticle */}
      {tile.isCandidateTarget && isActivationMode && (
        <g data-testid="activation-target-reticle" style={{ pointerEvents: "none" }}>
          <circle
            cx={tile.center.x}
            cy={tile.center.y}
            r={26}
            fill="none"
            stroke="#38bdf8"
            strokeWidth={2}
            strokeDasharray="4 2"
          />
          <line
            x1={tile.center.x - 32}
            y1={tile.center.y}
            x2={tile.center.x - 16}
            y2={tile.center.y}
            stroke="#38bdf8"
            strokeWidth={2}
          />
          <line
            x1={tile.center.x + 16}
            y1={tile.center.y}
            x2={tile.center.x + 32}
            y2={tile.center.y}
            stroke="#38bdf8"
            strokeWidth={2}
          />
          <line
            x1={tile.center.x}
            y1={tile.center.y - 32}
            x2={tile.center.x}
            y2={tile.center.y - 16}
            stroke="#38bdf8"
            strokeWidth={2}
          />
          <line
            x1={tile.center.x}
            y1={tile.center.y + 16}
            x2={tile.center.x}
            y2={tile.center.y + 32}
            stroke="#38bdf8"
            strokeWidth={2}
          />
          <rect
            x={tile.center.x - 24}
            y={tile.center.y - 36}
            width={48}
            height={13}
            rx={3}
            fill="#0f172a"
            stroke="#38bdf8"
            strokeWidth={1}
          />
          <text
            x={tile.center.x}
            y={tile.center.y - 27}
            textAnchor="middle"
            fill="#38bdf8"
            fontSize="8"
            fontWeight="bold"
            letterSpacing="1"
          >
            TARGET
          </text>
        </g>
      )}

      {/* Blocked by player's command token overlay */}
      {tile.commandTokens.some((ct) => ct.owner === viewerSeat) && (
        <g data-testid={`blocked-token-${tile.systemId}`} style={{ pointerEvents: "none" }}>
          <polygon points={tile.points} fill="rgba(15, 23, 42, 0.4)" />
          <rect
            x={tile.center.x - 32}
            y={tile.center.y - 36}
            width={64}
            height={13}
            rx={3}
            fill="#1e293b"
            stroke="#64748b"
            strokeWidth={1}
          />
          <text
            x={tile.center.x}
            y={tile.center.y - 27}
            textAnchor="middle"
            fill="#94a3b8"
            fontSize="8"
            fontWeight="bold"
            letterSpacing="0.5"
          >
            ACTIVATED
          </text>
        </g>
      )}

      {/* Tile Label / System Number */}
      <text
        x={tile.center.x}
        y={tile.center.y - 48}
        textAnchor="middle"
        fill="#cbd5e1"
        fontSize="11"
        fontWeight="bold"
        pointerEvents="none"
      >
        {sysId === "18"
          ? "Mecatol Rex"
          : tile.label
            ? tile.label.startsWith("#")
              ? tile.label
              : `#${sysId}`
            : `#${sysId}`}
      </text>

      {/* Anomaly badge: shape icon(s) plus the name */}
      <AnomalyBadge anomalies={tile.anomalies} cx={tile.center.x} y={tile.center.y - 33} />

      {/* Printed Wormholes */}
      {tile.wormholes.map((wh, wIdx) => {
        // Down the left edge of the hex, clear of the planet discs.
        const wX = tile.center.x - 47;
        const wY = tile.center.y - 30 + wIdx * 20;
        return (
          <g key={`wh-${wh.kind}-${wIdx}`}>
            <circle cx={wX} cy={wY} r="8" fill="#08111d" stroke={wh.color} strokeWidth="2" />
            <text
              x={wX}
              y={wY + 3.5}
              textAnchor="middle"
              fill={wh.color}
              fontSize="9"
              fontWeight="bold"
              pointerEvents="none"
            >
              {wh.symbol}
            </text>
          </g>
        );
      })}

      {/* View-Specific Overlays */}
      {activeOverlay === "economy" && <EconomyOverlay tile={tile} />}

      {activeOverlay === "space_combat" && <SpaceCombatOverlay tile={tile} players={players} />}

      {activeOverlay === "ground_combat" && (
        <GroundCombatOverlay
          tile={tile}
          players={players}
          onSelectTarget={onSelectTarget}
          onSelectSystem={onSelectSystem}
        />
      )}

      {activeOverlay === "tech_benefits" && (
        <TechBenefitsOverlay
          tile={tile}
          onSelectTarget={onSelectTarget}
          onSelectSystem={onSelectSystem}
        />
      )}

      {activeOverlay === "none" && (
        <StandardOverlay
          tile={tile}
          targetMode={targetMode}
          paymentMarks={paymentMarks}
          onSelectTarget={onSelectTarget}
          onSelectSystem={onSelectSystem}
        />
      )}

      <ExplorationMarkers tile={tile} />

      {/* Command Tokens */}
      {tile.commandTokens.map((ct, cIdx) => {
        const cx = tile.center.x + (cIdx - (tile.commandTokens.length - 1) / 2) * COMMAND_TOKEN_STEP;
        return (
          <g key={`cmd-${cIdx}`}>
            <title>{display(ct.owner).label} command token</title>
            <circle
              cx={cx}
              cy={tile.center.y + COMMAND_TOKEN_Y}
              r={COMMAND_TOKEN_RADIUS}
              fill={ct.color}
              stroke="#f8fafc"
              strokeWidth="1.5"
            />
            <text
              x={cx}
              y={tile.center.y + COMMAND_TOKEN_Y + COMMAND_TOKEN_RADIUS * 0.45}
              textAnchor="middle"
              fill={seatInk(ct.color)}
              fontSize={COMMAND_TOKEN_RADIUS * 1.3}
              pointerEvents="none"
            >
              {display(ct.owner).symbol}
            </text>
          </g>
        );
      })}
    </SvgButton>
  );
};
