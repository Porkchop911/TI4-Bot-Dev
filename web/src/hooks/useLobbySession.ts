import { useCallback, useEffect, useRef, useState } from "react";
import { decodeJoinResponse, decodeLobby } from "../protocol/decode.ts";
import { LobbyDto } from "../protocol/types.ts";
import { rememberNickname, validNickname } from "../protocol/nickname.ts";

export interface LobbySessionState {
  lobby: LobbyDto | null;
  playerId: string | null;
  error: string | null;
  loading: boolean;
  invalidCredential: boolean;
  pendingAction: string | null;
  setReady: (ready: boolean) => Promise<void>;
  start: () => Promise<void>;
  reorder: (slotIds: string[]) => Promise<void>;
  join: (nickname: string, playerId?: string) => Promise<string | undefined>;
  leave: () => Promise<boolean>;
  addBot: (password: string, nickname?: string, temperature?: number) => Promise<boolean>;
  removeBot: (playerId: string) => Promise<boolean>;
}

export function useLobbySession(gameId: string, playerSession?: string): LobbySessionState {
  const [lobby, setLobby] = useState<LobbyDto | null>(null);
  const [playerId, setPlayerId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [invalidCredential, setInvalidCredential] = useState(false);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const pending = useRef(false);
  const revision = useRef(0);
  const base = `/api/games/${encodeURIComponent(gameId)}/lobby`;
  const headers: Record<string, string> = playerSession
    ? { "x-ti4-player-session": playerSession }
    : {};

  useEffect(() => {
    let active = true;
    setInvalidCredential(false);
    const load = async () => {
      if (pending.current) return;
      const observed = revision.current;
      try {
        // The public lobby intentionally has no viewer field. Reconnect explicitly to
        // obtain the authenticated identity; spectators only make a read-only GET.
        const response = await fetch(
          playerSession ? `${base}/join` : base,
          playerSession
            ? {
                method: "POST",
                headers: { "Content-Type": "application/json", ...headers },
                body: JSON.stringify({ kind: "new" }),
              }
            : {},
        );
        if (!response.ok) {
          if (
            response.status === 403 &&
            playerSession &&
            active &&
            observed === revision.current &&
            !pending.current
          )
            setInvalidCredential(true);
          throw new Error(`Lobby request failed (${response.status})`);
        }
        const joined = playerSession ? decodeJoinResponse(await response.json(), gameId) : null;
        const next = joined?.lobby ?? decodeLobby(await response.json(), gameId);
        if (active && observed === revision.current && !pending.current) {
          setLobby(next);
          setPlayerId(joined?.player.id ?? null);
          setLoading(false);
        }
      } catch (cause) {
        if (active && observed === revision.current && !pending.current) {
          setError(String(cause));
          setLoading(false);
        }
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 2_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [gameId, playerSession]);

  const run = useCallback(
    async <T>(action: string, operation: () => Promise<T>, fallback: T): Promise<T> => {
      if (pending.current) return fallback;
      pending.current = true;
      revision.current++;
      setPendingAction(action);
      setError(null);
      try {
        return await operation();
      } catch (cause) {
        setError(String(cause));
        return fallback;
      } finally {
        pending.current = false;
        setPendingAction(null);
      }
    },
    [],
  );

  const mutate = useCallback(
    async (path: "ready" | "start" | "reorder", body?: object) =>
      run(
        path,
        async () => {
          const response = await fetch(`${base}/${path}`, {
            method: "POST",
            headers: { "Content-Type": "application/json", ...headers },
            body: body ? JSON.stringify(body) : undefined,
          });
          if (!response.ok) {
            throw new Error(`Lobby ${path} failed (${response.status}): ${await response.text()}`);
          }
          setLobby(decodeLobby(await response.json(), gameId));
          setError(null);
        },
        undefined,
      ),
    [base, gameId, playerSession, run],
  );

  const join = useCallback(
    async (nickname: string, takeoverId?: string) =>
      run(
        takeoverId ? "takeover" : "join",
        async () => {
          if (!validNickname(nickname))
            throw new Error(
              "Nickname must be 1–64 UTF-8 bytes, trimmed, without control or format characters.",
            );
          const response = await fetch(`${base}/join`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(
              takeoverId
                ? { kind: "takeover", player_id: takeoverId, nickname }
                : { kind: "new", nickname },
            ),
          });
          if (!response.ok)
            throw new Error(`Join failed (${response.status}): ${await response.text()}`);
          const joined = decodeJoinResponse(await response.json(), gameId);
          if (!joined.player_session) throw new Error("Join did not return a player session");
          setLobby(joined.lobby);
          setPlayerId(joined.player.id);
          setError(null);
          rememberNickname(nickname);
          return joined.player_session;
        },
        undefined,
      ),
    [base, gameId, run],
  );

  const leave = useCallback(async (): Promise<boolean> => {
    if (!playerSession) return false;
    return run(
      "leave",
      async () => {
        const response = await fetch(`${base}/leave`, { method: "POST", headers });
        if (!response.ok)
          throw new Error(`Leave lobby failed (${response.status}): ${await response.text()}`);
        // Validate the successful server response before removing the only local credential.
        decodeLobby(await response.json(), gameId);
        setError(null);
        return true;
      },
      false,
    );
  }, [base, gameId, playerSession, run]);

  const addBot = useCallback(
    async (password: string, nickname?: string, temperature?: number): Promise<boolean> => {
      if (!playerSession) return false;
      return run(
        "add_bot",
        async () => {
          const response = await fetch(`${base}/add-bot`, {
            method: "POST",
            headers: { "Content-Type": "application/json", ...headers },
            body: JSON.stringify({ password, nickname: nickname || undefined, temperature }),
          });
          if (!response.ok) {
            const msg = await response.text();
            throw new Error(`Add bot failed (${response.status}): ${msg}`);
          }
          const updated = decodeLobby(await response.json(), gameId);
          setLobby(updated);
          setError(null);
          return true;
        },
        false,
      );
    },
    [base, gameId, headers, playerSession, run],
  );

  const removeBot = useCallback(
    async (targetPlayerId: string): Promise<boolean> => {
      if (!playerSession) return false;
      return run(
        "remove_bot",
        async () => {
          const response = await fetch(`${base}/remove-bot`, {
            method: "POST",
            headers: { "Content-Type": "application/json", ...headers },
            body: JSON.stringify({ player_id: targetPlayerId }),
          });
          if (!response.ok) {
            const msg = await response.text();
            throw new Error(`Remove bot failed (${response.status}): ${msg}`);
          }
          const updated = decodeLobby(await response.json(), gameId);
          setLobby(updated);
          setError(null);
          return true;
        },
        false,
      );
    },
    [base, gameId, headers, playerSession, run],
  );

  return {
    lobby,
    playerId,
    error,
    loading,
    invalidCredential,
    pendingAction,
    setReady: (ready) => mutate("ready", { ready }),
    start: () => mutate("start"),
    reorder: (slot_ids) => mutate("reorder", { slot_ids }),
    join,
    leave,
    addBot,
    removeBot,
  };
}
