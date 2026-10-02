import React, { useState, useRef } from "react";
import { BoardView, PlayerView, PendingChoiceDto } from "../protocol/types.ts";
import {
  buildBoardPresentationModel,
  getPlayerColor,
  PLAYER_PALETTE,
} from "../presentation/boardPresentation.ts";
import { SystemInspector } from "./SystemInspector.tsx";
import { Tooltip } from "../primitives/index.ts";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { seatStyle } from "../presentation/playerDisplay.ts";
import { MapOverlayMode } from "../presentation/mapOverlays.ts";
import { MapOverlayToolbar } from "./MapOverlayToolbar.tsx";
import { BoardTile } from "./board/BoardTile.tsx";
import { BoardTooltip, HoveredTileInfo } from "./board/BoardTooltip.tsx";
import { MovementVectorsOverlay } from "./board/MovementVectorsOverlay.tsx";

export { getPlayerColor, PLAYER_PALETTE };
export type { MapOverlayMode };

export interface BoardProps {
  board: BoardView;
  seatingOrder: string[];
  players?: readonly PlayerView[];
  pendingChoice?: PendingChoiceDto | null;
  viewerSeat?: string | null;
  selectedSystemId?: string | null;
  onSelectSystem?: (systemId: string | null) => void;
  onSelectTarget?: (systemId: string, planetId?: string) => void;
  onSelectOptionId?: (optionId: string) => void;
  overlayMode?: MapOverlayMode;
  onOverlayModeChange?: (mode: MapOverlayMode) => void;
}

export const Board: React.FC<BoardProps> = ({
  board,
  seatingOrder,
  players = [],
  pendingChoice = null,
  viewerSeat = null,
  selectedSystemId: controlledSelectedSystemId,
  onSelectSystem,
  onSelectTarget,
  onSelectOptionId,
  overlayMode: controlledOverlayMode,
  onOverlayModeChange,
}) => {
  const display = usePlayerIdentity();
  const [uncontrolledSelectedSystemId, setUncontrolledSelectedSystemId] = useState<string | null>(
    null,
  );
  const selectedSystemId =
    controlledSelectedSystemId === undefined
      ? uncontrolledSelectedSystemId
      : controlledSelectedSystemId;
  const setSelectedSystemId = (id: string | null) => {
    if (controlledSelectedSystemId === undefined) {
      setUncontrolledSelectedSystemId(id);
    }
    onSelectSystem?.(id);
  };

  const [uncontrolledOverlayMode, setUncontrolledOverlayMode] = useState<MapOverlayMode>("none");
  const activeOverlay =
    controlledOverlayMode !== undefined ? controlledOverlayMode : uncontrolledOverlayMode;
  const handleSelectOverlay = (mode: MapOverlayMode) => {
    if (controlledOverlayMode === undefined) {
      setUncontrolledOverlayMode(mode);
    }
    onOverlayModeChange?.(mode);
  };

  const [hoveredTile, setHoveredTile] = useState<HoveredTileInfo | null>(null);

  // Pan and zoom state
  const [viewTransform, setViewTransform] = useState({ x: 0, y: 0, scale: 1 });
  const [isPanning, setIsPanning] = useState(false);
  const startPanRef = useRef({ x: 0, y: 0 });

  // Pure presentation derivation
  const presentation = buildBoardPresentationModel(
    board,
    seatingOrder,
    players,
    pendingChoice,
    viewerSeat,
    selectedSystemId,
  );

  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.button === 0) {
      setIsPanning(true);
      startPanRef.current = { x: e.clientX - viewTransform.x, y: e.clientY - viewTransform.y };
    }
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (isPanning) {
      setViewTransform((prev) => ({
        ...prev,
        x: e.clientX - startPanRef.current.x,
        y: e.clientY - startPanRef.current.y,
      }));
    }
  };

  const handlePointerUp = () => setIsPanning(false);

  const zoomIn = () =>
    setViewTransform((prev) => ({ ...prev, scale: Math.min(prev.scale * 1.25, 2.5) }));
  const zoomOut = () =>
    setViewTransform((prev) => ({ ...prev, scale: Math.max(prev.scale / 1.25, 0.5) }));
  const resetView = () => setViewTransform({ x: 0, y: 0, scale: 1 });

  return (
    <div
      className="board-container"
      data-testid="board-viewport"
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        overflow: "hidden",
        background: "#090d16",
        userSelect: "none",
      }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
    >
      {/* Pan / Zoom Control Overlay & Map Overlay Selector */}
      <div
        className="board-controls"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <div style={{ display: "flex", gap: 6 }}>
          <Tooltip content="Zoom In">
            <button
              type="button"
              onClick={zoomIn}
              title="Zoom In"
              className="button button--secondary button--icon"
              style={{ fontSize: 16 }}
            >
              +
            </button>
          </Tooltip>
          <Tooltip content="Zoom Out">
            <button
              type="button"
              onClick={zoomOut}
              title="Zoom Out"
              className="button button--secondary button--icon"
              style={{ fontSize: 16 }}
            >
              −
            </button>
          </Tooltip>
          <Tooltip content="Reset Pan & Zoom">
            <button
              type="button"
              onClick={resetView}
              title="Reset Pan & Zoom"
              className="button button--secondary button--icon"
              style={{ fontSize: 14 }}
            >
              ⟲
            </button>
          </Tooltip>
        </div>

        <MapOverlayToolbar activeMode={activeOverlay} onSelectMode={handleSelectOverlay} />
      </div>

      <div className="board-seat-legend" aria-label="Player positions">
        {seatingOrder.map((id, index) => (
          <span key={id} style={{ borderColor: seatStyle(index + 1).color }}>
            <SeatBadge position={index + 1} /> {display(id).label}
          </span>
        ))}
      </div>

      <svg
        viewBox={presentation.viewBox}
        onClick={(e) => {
          if (e.target === e.currentTarget) setSelectedSystemId(null);
        }}
        style={{
          width: "100%",
          height: "100%",
          display: "block",
          cursor: isPanning ? "grabbing" : "grab",
        }}
        data-testid="ti4-board-svg"
      >
        <defs>
          <filter id="glow" x="-20%" y="-20%" width="140%" height="140%">
            <feGaussianBlur stdDeviation="4" result="blur" />
            <feComposite in="SourceGraphic" in2="blur" operator="over" />
          </filter>
          <filter id="target-glow" x="-30%" y="-30%" width="160%" height="160%">
            <feGaussianBlur stdDeviation="6" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          <marker
            id="vector-arrow"
            viewBox="0 0 10 10"
            refX="8"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M 0 1 L 10 5 L 0 9 z" fill="#4ade80" />
          </marker>
        </defs>

        <g
          transform={`translate(${viewTransform.x}, ${viewTransform.y}) scale(${viewTransform.scale})`}
          onClick={(e) => {
            if (e.target === e.currentTarget) setSelectedSystemId(null);
          }}
        >
          {presentation.tiles.map((tile, idx) => (
            <BoardTile
              key={`hex-${tile.systemId}-${idx}`}
              tile={tile}
              isSelected={selectedSystemId === tile.systemId}
              activeOverlay={activeOverlay}
              viewerSeat={viewerSeat}
              isActivationMode={presentation.targets.isActivationMode}
              players={players}
              onSelectTarget={onSelectTarget}
              onSelectOptionId={onSelectOptionId}
              onSelectSystem={setSelectedSystemId}
              onMouseEnter={() => {
                setHoveredTile({
                  systemId: tile.systemId,
                  label: tile.label,
                  anomalies: tile.anomalies,
                  wormholes: tile.wormholes.map((w) => w.kind),
                  planets: tile.planets.map((p) => ({
                    label: p.label,
                    resources: p.resources,
                    influence: p.influence,
                    owner: p.controlledBy,
                  })),
                  unitsCount: tile.totalUnits,
                  tokensCount: tile.commandTokens.length,
                });
              }}
              onMouseLeave={() => setHoveredTile(null)}
            />
          ))}

          {/* Movement Vector Overlays */}
          <MovementVectorsOverlay vectors={presentation.targets.movementVectors} />
        </g>
      </svg>

      {/* Selected System Inspector */}
      {presentation.selectedSystem && (
        <SystemInspector
          system={presentation.selectedSystem}
          onClose={() => setSelectedSystemId(null)}
        />
      )}

      {/* Hover Info Tooltip */}
      {hoveredTile && !presentation.selectedSystem && (
        <BoardTooltip
          hoveredTile={hoveredTile}
          tilePresentation={presentation.tiles.find((t) => t.systemId === hoveredTile.systemId)}
          activeOverlay={activeOverlay}
          seatingOrder={seatingOrder}
          players={players}
        />
      )}
    </div>
  );
};
