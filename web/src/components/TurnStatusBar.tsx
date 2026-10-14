import React from "react";
import { PublicTurnStatus, GameView } from "../protocol/types.ts";
import { ConnectionStatus } from "../hooks/useGameSession.ts";
import { useParticipantText, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import "./MobileTopMenu.css";

export interface TurnStatusBarProps {
  status: PublicTurnStatus | null;
  view: GameView | null;
  gameVersion: number;
  connectionStatus: ConnectionStatus;
  userSeat?: string;
  /** A phone: one slim row (connection dot, round and phase, a short status, version) and `trailing`. */
  compact?: boolean;
  /** The phone menu button, at the end of the compact row. */
  trailing?: React.ReactNode;
}

/** Engine stage ids ("activate_system", "vote_exhaust_planet") as human text; never show the raw id. */
export function stageLabel(stage: string): string {
  const words = stage
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/[_\-\s]+/g, " ")
    .trim()
    .toLowerCase();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : "";
}

const withStage = (text: string, stage: string) => {
  const label = stageLabel(stage);
  return label ? `${text} (${label})` : text;
};

const CONNECTION_COLOR: Record<ConnectionStatus, string> = {
  connected: "#10b981",
  connecting: "#f59e0b",
  disconnected: "#ef4444",
  error: "#ef4444",
};

/** The phone bar's short status: who is up, no stage or round (those are in the menu). */
export function shortStatusText(
  status: PublicTurnStatus | null,
  userSeat: string | undefined,
  label: (id: string) => string,
): string {
  if (!status) return "Starting…";
  switch (status.kind) {
    case "active_turn":
      return userSeat && status.player === userSeat ? "Your turn" : `${label(status.player)}'s turn`;
    case "waiting_for_decision":
      return userSeat && status.seat === userSeat ? "YOUR TURN" : `Waiting: ${label(status.seat)}`;
    case "waiting_for_reactions":
      return "Waiting for reactions";
    case "phase_transition":
      return `Phase: ${status.phase}`;
    case "game_over":
      return `Game Over! Winner: ${status.winner ? label(status.winner) : "Draw"}`;
  }
}

function fullStatusText(
  status: PublicTurnStatus | null,
  userSeat: string | undefined,
  display: ReturnType<typeof usePlayerIdentity>,
): string {
  if (!status) return "Initializing game...";
  switch (status.kind) {
    case "active_turn":
      return `Active Turn: ${display(status.player).label} (Round ${status.round}, ${status.phase})`;
    case "waiting_for_decision": {
      const isYou = userSeat && status.seat === userSeat;
      return isYou
        ? withStage("YOUR TURN: Awaiting your choice", status.stage)
        : withStage(`Waiting for ${display(status.seat).label}`, status.stage);
    }
    case "waiting_for_reactions":
      return "Waiting for reactions";
    case "phase_transition":
      return `Phase Transition: ${status.phase} (Round ${status.round})`;
    case "game_over":
      return `Game Over! Winner: ${status.winner ? display(status.winner).label : "Draw"}`;
  }
}

/** The status facts the phone menu shows: the full line, round, phase, speaker, version and link. */
export const TurnStatusDetails: React.FC<
  Pick<TurnStatusBarProps, "status" | "view" | "gameVersion" | "connectionStatus" | "userSeat">
> = ({ status, view, gameVersion, connectionStatus, userSeat }) => {
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const yours =
    status?.kind === "waiting_for_decision" && Boolean(userSeat) && status.seat === userSeat;
  return (
    <>
      <div className="top-menu__status-line" data-you={yours}>
        {present(fullStatusText(status, userSeat, display))}
      </div>
      {view && (
        <div className="top-menu__status-facts">
          Round {view.round} • <span style={{ textTransform: "capitalize" }}>{view.phase} phase</span>{" "}
          • Speaker:{" "}
          <strong style={{ color: display(view.speaker).color }}>{display(view.speaker).label}</strong>
        </div>
      )}
      <div className="top-menu__status-facts">
        <span style={{ textTransform: "capitalize" }}>{connectionStatus}</span> • game v{gameVersion}
      </div>
    </>
  );
};

export const TurnStatusBar: React.FC<TurnStatusBarProps> = ({
  status,
  view,
  gameVersion,
  connectionStatus,
  userSeat,
  compact = false,
  trailing,
}) => {
  const display = usePlayerIdentity();
  const present = useParticipantText();
  if (compact) {
    const yours =
      status?.kind === "waiting_for_decision" && Boolean(userSeat) && status.seat === userSeat;
    return (
      <header
        data-testid="turn-status-bar"
        data-compact="true"
        aria-live="polite"
        className="mobile-top-bar"
      >
        <div
          data-testid="connection-indicator"
          data-status={connectionStatus}
          title={connectionStatus}
          style={{ display: "flex", alignItems: "center" }}
        >
          <span
            className="mobile-top-bar__dot"
            style={{ background: CONNECTION_COLOR[connectionStatus] }}
          />
          <span className="visually-hidden">{connectionStatus}</span>
        </div>
        {view && (
          <span className="mobile-top-bar__round">
            R{view.round} · {view.phase}
          </span>
        )}
        <div
          data-testid="turn-status-banner"
          data-status-kind={status?.kind ?? "unknown"}
          className="mobile-top-bar__banner"
          title={present(fullStatusText(status, userSeat, display))}
          style={{
            background: yours ? "#b45309" : "#1e293b",
            color: yours ? "#fef3c7" : "#38bdf8",
          }}
        >
          {present(shortStatusText(status, userSeat, (id) => display(id).label))}
        </div>
        <div data-testid="game-version" className="mobile-top-bar__version">
          v{gameVersion}
        </div>
        {trailing}
      </header>
    );
  }
  const getStatusText = () => {
    if (!status) return "Initializing game...";
    switch (status.kind) {
      case "active_turn":
        return `Active Turn: ${display(status.player).label} (Round ${status.round}, ${status.phase})`;
      case "waiting_for_decision": {
        const isYou = userSeat && status.seat === userSeat;
        return isYou
          ? withStage("YOUR TURN: Awaiting your choice", status.stage)
          : withStage(`Waiting for ${display(status.seat).label}`, status.stage);
      }
      case "waiting_for_reactions":
        // The same line for a real window and a bluff hold; it never names a seat.
        return "Waiting for reactions";
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
        flexWrap: "wrap",
        rowGap: 6,
        padding: "10px 20px",
        background: "#0f172a",
        borderBottom: "1px solid #1e293b",
        color: "#f8fafc",
        fontSize: 14,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "6px 16px", flexWrap: "wrap" }}>
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
