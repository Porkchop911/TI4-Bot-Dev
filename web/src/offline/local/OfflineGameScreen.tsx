import React, { useEffect, useMemo } from "react";
import { setActiveOfflineGame } from "../registry.ts";
import type { OfflineFeedbackLocal, OfflineGameHandle } from "../types.ts";
import type { GameTransport } from "../../protocol/transport.ts";
import type { LobbyDto, ViewerRole } from "../../protocol/types.ts";
import type { LocalTransport } from "./LocalTransport.ts";
import "../offline.css";

export interface OfflineGameViewProps {
  gameId: string;
  lobby: LobbyDto;
  viewer: ViewerRole;
  createTransport: () => GameTransport;
}

/**
 * The roster the game screen reads names and seat colors from. There is no lobby offline: the
 * human is the "host" of a table of rule bots; the nickname never leaves this device.
 */
export function offlineLobby(
  gameId: string,
  seatingOrder: readonly string[],
  humanSeat: string,
  nickname: string,
): LobbyDto {
  return {
    game_id: gameId,
    phase: "running",
    lobby_version: 1,
    host_player_id: humanSeat,
    slots: seatingOrder.map((seat, index) => ({
      slot_id: `slot_${seat}`,
      position: index + 1,
      occupant: seat,
      nickname: seat === humanSeat ? nickname : `Bot ${index + 1}`,
      connected: true,
      can_take_over: false,
      ...(seat === humanSeat ? {} : { bot: "smart" }),
    })),
  };
}

/**
 * /offline/games/<id>: the real game screen on the local transport. `renderGame` is the app's
 * own game view (passed in, so this file does not import the app), the banner sits above it.
 */
export const OfflineGameScreen: React.FC<{
  handle: OfflineGameHandle;
  banner: React.ReactNode;
  renderGame: (props: OfflineGameViewProps) => React.ReactNode;
}> = ({ handle, banner, renderGame }) => {
  const transport = handle.transport as LocalTransport;
  const snapshot = transport.getState().snapshot;
  const seat = snapshot?.viewer.role === "player" ? snapshot.viewer.seat : null;
  const seating = snapshot?.view.seating_order;
  const lobby = useMemo(
    () => (seat && seating ? offlineLobby(handle.gameId, seating, seat, transport.nickname) : null),
    [handle.gameId, seat, seating, transport],
  );

  // Feedback sent from here goes to the offline queue with what replays the game.
  useEffect(() => {
    setActiveOfflineGame({
      gameId: handle.gameId,
      local: () => {
        const save = transport.latestSave();
        if (!save) throw new Error("The game has not started");
        return {
          seed: save.seed,
          answers: save.answers,
          engine: { build: save.engine.build, content_digest: save.engine.content_digest },
          wasm_sha256: transport.wasmSha256,
          save: save as unknown as OfflineFeedbackLocal["save"],
        };
      },
    });
    return () => setActiveOfflineGame(null);
  }, [handle.gameId, transport]);

  if (!seat || !lobby)
    return (
      <main className="lobby-page" data-testid="offline-game-loading">
        {banner}
        <section className="panel lobby-panel">Loading the game…</section>
      </main>
    );
  return (
    <div className="offline-game-frame" data-testid="offline-game">
      {banner}
      {renderGame({
        gameId: handle.gameId,
        lobby,
        viewer: { role: "player", seat, playerSession: "offline" },
        createTransport: () => transport,
      })}
    </div>
  );
};
