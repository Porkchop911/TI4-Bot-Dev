import React from "react";
import { PublicTurnStatus, GameView } from "../protocol/types.ts";
import { ConnectionStatus } from "../hooks/useGameSession.ts";
import { useParticipantText, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";

export interface TurnStatusBarProps {
  status: PublicTurnStatus | null;
  view: GameView | null;
  gameVersion: number;
  connectionStatus: ConnectionStatus;
  userSeat?: string;
}

export const TurnStatusBar: React.FC<TurnStatusBarProps> = ({
  status,
  view,
  gameVersion,
  connectionStatus,
  userSeat,
}) => {
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const getStatusText = () => {
    if (!status) return "Initializing game...";
    switch (status.kind) {
      case "active_turn":
        return `Active Turn: ${display(status.player).label} (Round ${status.round}, ${status.phase})`;
      case "waiting_for_decision": {
        const isYou = userSeat && status.seat === userSeat;
        return isYou
          ? `YOUR TURN: Awaiting your choice (${status.stage})`
          : `Waiting for ${display(status.seat).label} (${status.stage})`;
      }
      case "phase_transition":
        return `Phase Transition: ${status.phase} (Round ${status.round})`;
      case "game_over":
        return `Game Over! Winner: ${status.winner ? display(status.winner).label : "Draw"}`;
    }
  };

  const getStatusBadgeColor = () => {
    switch (connectionStatus) {
      case "connected":
        return "#10b981";
      case "connecting":
        return "#f59e0b";
      case "disconnected":
      case "error":
        return "#ef4444";
    }
  };

  return (
    <header
      data-testid="turn-status-bar"
      aria-live="polite"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "10px 20px",
        background: "#0f172a",
        borderBottom: "1px solid #1e293b",
        color: "#f8fafc",
        fontSize: 14,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
        {/* Connection status indicator */}
        <div
          data-testid="connection-indicator"
          data-status={connectionStatus}
          style={{ display: "flex", alignItems: "center", gap: 6 }}
        >
          <span
            style={{
              width: 10,
              height: 10,
              borderRadius: "50%",
              background: getStatusBadgeColor(),
              display: "inline-block",
            }}
          />
          <span style={{ fontSize: 12, color: "#94a3b8", textTransform: "capitalize" }}>
            {connectionStatus}
          </span>
        </div>

        {/* Game version */}
        <div data-testid="game-version" style={{ color: "#64748b", fontSize: 12 }}>
          v{gameVersion}
        </div>

        {view && (
          <div style={{ display: "flex", gap: 12, color: "#94a3b8" }}>
            <span>Round {view.round}</span>
            <span>•</span>
            <span style={{ textTransform: "capitalize" }}>{view.phase} Phase</span>
            <span>•</span>
            <span>
              Speaker:{" "}
              <strong style={{ color: display(view.speaker).color }}>
                {display(view.speaker).label}
              </strong>
            </span>
          </div>
        )}
      </div>

      {/* Main turn status banner */}
      <div
        data-testid="turn-status-banner"
        data-status-kind={status?.kind ?? "unknown"}
        style={{
          fontWeight: "bold",
          padding: "4px 14px",
          borderRadius: 6,
          background:
            status?.kind === "waiting_for_decision" && userSeat && status.seat === userSeat
              ? "#b45309"
              : "#1e293b",
          color:
            status?.kind === "waiting_for_decision" && userSeat && status.seat === userSeat
              ? "#fef3c7"
              : "#38bdf8",
          border:
            status?.kind === "waiting_for_decision" && userSeat && status.seat === userSeat
              ? "1px solid #f59e0b"
              : "1px solid #334155",
        }}
      >
        {present(getStatusText())}
      </div>
    </header>
  );
};
