import React, { useEffect, useMemo, useState, useRef, useId } from "react";
import { createPortal } from "react-dom";
import { useCompactLayout } from "../hooks/useCompactLayout.ts";
import { useMobileMenuSlot } from "../presentation/MobileMenuSlot.tsx";
import { BoardView, PlayerView, PendingChoiceDto } from "../protocol/types.ts";
import {
  buildBoardPresentationModel,
  getPlayerColor,
  PLAYER_PALETTE,
} from "../presentation/boardPresentation.ts";
import { SystemInspector, type InspectorTab } from "./SystemInspector.tsx";
import { SystemReachTab } from "./SystemReachTab.tsx";
import { useReachQuery } from "../hooks/useReachQuery.ts";
import { groupReach, reachVectors } from "../presentation/reachView.ts";
import type { ReachReply } from "../protocol/client.ts";
import { Tooltip } from "../primitives/index.ts";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { seatStyle } from "../presentation/playerDisplay.ts";
import { boardStageStyle } from "./board/boardLayout.ts";
import { MapOverlayMode } from "../presentation/mapOverlays.ts";
import { MapOverlayToolbar } from "./MapOverlayToolbar.tsx";
import { useBoardPrepSlot } from "../presentation/BoardPrepSlot.tsx";
import { useBoardChromeOffset } from "../hooks/useBoardChromeOffset.ts";
import { BoardTile } from "./board/BoardTile.tsx";
import { BoardTooltip, HoveredTileInfo } from "./board/BoardTooltip.tsx";
import { MovementVectorsOverlay } from "./board/MovementVectorsOverlay.tsx";
import {
  EMPTY_PAYMENT_DRAFT,
  derivePaymentMarks,
  derivePaymentOffer,
  type PaymentMark,
} from "../presentation/paymentDraft.ts";
import { useSharedPaymentDraft } from "../presentation/PaymentDraftContext.tsx";
import { useSharedTokenDraft } from "../presentation/CommandTokenDraftContext.tsx";
import { useSharedReadyDraft } from "../presentation/ReadyPlanetsDraftContext.tsx";

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
  /** Read-only "who can go here" query; without it the system overview has no such tab. */
  queryReach?: (systemId: string) => Promise<ReachReply>;
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
  queryReach,
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
  const chromeRef = useRef<HTMLDivElement | null>(null);
  useBoardChromeOffset(chromeRef);
  const prepSlot = useBoardPrepSlot();
  // A phone with the menu sheet: the map keeps its whole height, the controls move to the sheet.
  const menuSlot = useMobileMenuSlot();
  const compactChrome = useCompactLayout() && menuSlot !== null;
  const helpId = useId();

  // Leadership's purchase: while tokens are bought the planets that can pay are the map's targets.
  const tokenPayment = useSharedTokenDraft()?.mapPayment ?? null;
  const tokenPaymentPlanets = useMemo(
    () => tokenPayment?.offer.planets.map((planet) => planet.planetId),
    [tokenPayment?.offer],
  );

  // Pure presentation derivation
  const presentation = buildBoardPresentationModel(
    board,
    seatingOrder,
    players,
    pendingChoice,
    viewerSeat,
    selectedSystemId,
    tokenPaymentPlanets,
  );

  // "Who can go here": the inspector's second tab asks the engine and draws the answer as arrows.
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("details");
  const [reachFocus, setReachFocus] = useState<string | null>(null);
  useEffect(() => setReachFocus(null), [selectedSystemId]);
  const reachActive = Boolean(queryReach) && inspectorTab === "reach" && Boolean(selectedSystemId);
  const reach = useReachQuery(queryReach, selectedSystemId, reachActive, board);
  const reachGroups = useMemo(
    () => groupReach(reach.status === "ready" ? reach.origins : [], viewerSeat),
    [reach, viewerSeat],
  );
  const reachArrows = useMemo(() => {
    if (!reachActive || !selectedSystemId || reach.status !== "ready") return [];
    const centers = new Map(presentation.tiles.map((t) => [t.systemId, t.center] as const));
    return reachVectors(reachGroups, selectedSystemId, centers, reachFocus);
  }, [reachActive, selectedSystemId, reach.status, reachGroups, reachFocus, presentation.tiles]);

  // Only the standard overlay draws clickable planets for every system, so a pending planet pick
  // shows it regardless of the chosen overlay; the preference returns afterwards.
  const isPlanetTargeting =
    presentation.targets.targetMode === "planet" || presentation.targets.targetMode === "payment";
  const effectiveOverlay: MapOverlayMode = isPlanetTargeting ? "none" : activeOverlay;

  // While paying, each payable planet shows what it is worth and whether it is staged.
  const sharedDraft = useSharedPaymentDraft();
  // Diplomacy: the exhausted planets that can be readied show their values and whether they are picked.
  const readyDraft = useSharedReadyDraft();
  const readyMarks = useMemo(() => {
    const view = readyDraft?.view;
    if (!view || !pendingChoice || pendingChoice.actor !== viewerSeat) return undefined;
    const marks = new Map<string, PaymentMark>();
    for (const planet of view.planets) {
      marks.set(planet.id, {
        planetId: planet.id,
        optionId: planet.id,
        worth: planet.influence ?? 0,
        unit: "I",
        staged: readyDraft.selected.includes(planet.id),
        resources: planet.resources ?? undefined,
        readies: true,
      });
    }
    return marks;
  }, [readyDraft?.view, readyDraft?.selected, pendingChoice, viewerSeat]);
  const paymentMarks = useMemo(
    () =>
      readyMarks ??
      (presentation.targets.targetMode === "payment" && tokenPayment
        ? derivePaymentMarks(tokenPayment.offer, tokenPayment.draft)
        : presentation.targets.targetMode === "payment" && pendingChoice
          ? derivePaymentMarks(
              derivePaymentOffer(pendingChoice),
              sharedDraft?.draft ?? EMPTY_PAYMENT_DRAFT,
            )
          : undefined),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [presentation.targets.targetMode, pendingChoice, sharedDraft?.draft, tokenPayment, readyMarks],
  );

  // Touch: one finger pans, two fingers pinch-zoom the map only (touch-action: none keeps the
  // browser from zooming the whole page, dialogs included). Pointers are tracked by id.
  const pointersRef = useRef(new Map<number, { x: number; y: number }>());
  const pinchRef = useRef<{ distance: number; scale: number } | null>(null);
  const pinchDistance = () => {
    const [a, b] = [...pointersRef.current.values()];
    return Math.hypot(a.x - b.x, a.y - b.y);
  };

  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    pointersRef.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointersRef.current.size === 2) {
      setIsPanning(false);
      pinchRef.current = { distance: pinchDistance() || 1, scale: viewTransform.scale };
    } else if (pointersRef.current.size === 1) {
      setIsPanning(true);
      startPanRef.current = { x: e.clientX - viewTransform.x, y: e.clientY - viewTransform.y };
    }
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (pointersRef.current.has(e.pointerId)) {
      pointersRef.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    }
    if (pointersRef.current.size >= 2 && pinchRef.current) {
      const { distance, scale } = pinchRef.current;
      const next = Math.min(2.5, Math.max(0.5, (scale * pinchDistance()) / distance));
      setViewTransform((prev) => ({ ...prev, scale: next }));
      return;
    }
    if (isPanning) {
      setViewTransform((prev) => ({
        ...prev,
        x: e.clientX - startPanRef.current.x,
        y: e.clientY - startPanRef.current.y,
      }));
    }
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    pointersRef.current.delete(e.pointerId);
    pinchRef.current = null;
    // The finger left over after a pinch starts a fresh pan from where it is.
    const rest = [...pointersRef.current.values()][0];
    if (rest) {
      setIsPanning(true);
      startPanRef.current = { x: rest.x - viewTransform.x, y: rest.y - viewTransform.y };
    } else {
      setIsPanning(false);
    }
  };

  const zoomIn = () =>
    setViewTransform((prev) => ({ ...prev, scale: Math.min(prev.scale * 1.25, 2.5) }));
  const zoomOut = () =>
    setViewTransform((prev) => ({ ...prev, scale: Math.max(prev.scale / 1.25, 0.5) }));
  const resetView = () => setViewTransform({ x: 0, y: 0, scale: 1 });

  const zoomControls = (
    <div style={{ display: "flex", gap: 6 }}>
      <Tooltip content="Zoom In" position="bottom">
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
      <Tooltip content="Zoom Out" position="bottom">
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
      <Tooltip content="Reset Pan & Zoom" position="bottom">
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
  );
  const overlayToolbar = (
    <MapOverlayToolbar
      activeMode={effectiveOverlay}
      onSelectMode={handleSelectOverlay}
      disabledReason={isPlanetTargeting ? "Overlays are paused while you choose a planet" : undefined}
    />
  );
  const seatLegend = (
    <div className="board-seat-legend" aria-label="Player positions">
      {seatingOrder.map((id, index) => (
        <span key={id} style={{ borderColor: seatStyle(index + 1).color }}>
          <SeatBadge position={index + 1} /> {display(id).label}
        </span>
      ))}
    </div>
  );

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
        touchAction: "none",
        display: "flex",
        flexDirection: "column",
      }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
    >
      {/* The toolbar and seat legend take their own row, so the map is fitted below them. On a phone
          that row holds nothing but the prepare chip: the controls are in the menu sheet. */}
      <div
        ref={chromeRef}
        className={`board-chrome${compactChrome ? " board-chrome--compact" : ""}`}
        data-testid="board-chrome"
      >
        {!compactChrome && (
          <div className="board-controls" style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
            {zoomControls}
            {overlayToolbar}
          </div>
        )}
        {!compactChrome && seatLegend}
        {prepSlot && <div ref={prepSlot.setSlot} className="board-chrome__prep" data-testid="board-prep-slot" />}
      </div>
      {compactChrome &&
        menuSlot?.slot &&
        createPortal(
          <>
            <section className="top-menu__section" aria-label="Zoom">
              <h3>Zoom</h3>
              <div className="board-controls">{zoomControls}</div>
            </section>
            <section className="top-menu__section" aria-label="Map view">
              <h3>View</h3>
              {overlayToolbar}
            </section>
            <section className="top-menu__section" aria-label="Who's who">
              <h3>Who&rsquo;s who</h3>
              {seatLegend}
            </section>
          </>,
          menuSlot.slot,
        )}

      <div className="board-stage" data-testid="board-stage" style={boardStageStyle(pendingChoice != null)}>
        <p id={helpId} className="visually-hidden">
          Drag the map to pan it, pinch with two fingers to zoom it. You can also use the zoom buttons to zoom in and out, and the reset button to
          return to the full view.
        </p>
      <svg
        role="group"
        aria-label="Galaxy map"
        aria-describedby={helpId}
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
          <marker
            id="vector-arrow-enemy"
            viewBox="0 0 10 10"
            refX="8"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M 0 1 L 10 5 L 0 9 z" fill="#f87171" />
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
              activeOverlay={effectiveOverlay}
              viewerSeat={viewerSeat}
              isActivationMode={presentation.targets.isActivationMode}
              targetMode={presentation.targets.targetMode}
              paymentMarks={paymentMarks}
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
          {reachArrows.length > 0 && <MovementVectorsOverlay vectors={reachArrows} />}
        </g>
      </svg>
      </div>

      {/* Selected System Inspector */}
      {presentation.selectedSystem && (
        <SystemInspector
          system={presentation.selectedSystem}
          onClose={() => setSelectedSystemId(null)}
          viewerSeat={viewerSeat}
          seatingOrder={seatingOrder}
          reach={
            queryReach
              ? {
                  tab: inspectorTab,
                  onTab: (tab) => {
                    setInspectorTab(tab);
                    setReachFocus(null);
                  },
                  content: (
                    <SystemReachTab
                      systemId={presentation.selectedSystem.systemId}
                      board={board}
                      viewerSeat={viewerSeat}
                      state={reach}
                      focusKey={reachFocus}
                      onFocus={setReachFocus}
                    />
                  ),
                }
              : undefined
          }
        />
      )}

      {/* Hover Info Tooltip */}
      {hoveredTile && !presentation.selectedSystem && (
        <BoardTooltip
          hoveredTile={hoveredTile}
          tilePresentation={presentation.tiles.find((t) => t.systemId === hoveredTile.systemId)}
          activeOverlay={effectiveOverlay}
          seatingOrder={seatingOrder}
          players={players}
        />
      )}
    </div>
  );
};
