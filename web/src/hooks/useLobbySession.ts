import { useCallback, useEffect, useRef, useState } from "react";
import { isGone, joinLobby, lobbyPath, lobbyPost } from "@ti4/shared/net/api.ts";
import { decodeJoinResponse, decodeLobby } from "../protocol/decode.ts";
import { InProcessBotKind, LobbyDto, MapChoice } from "../protocol/types.ts";
import { rememberNickname, validNickname } from "../protocol/nickname.ts";
import { describeError, onResume } from "../protocol/resilience.ts";

export interface LobbySessionState {
  lobby: LobbyDto | null;
  playerId: string | null;
  /** The last failed ACTION (start, join ...); cleared by the next action or `dismissError`. */
  error: string | null;
  /** True after LOBBY_POLL_FAILURE_LIMIT consecutive failed background polls; clears on the next success. */
  connectionLost: boolean;
  /** The server answered 404: the game does not exist (any more). */
  gone: boolean;
  /** Poll again right now (the Retry button; also what a resume does). */
  retry: () => void;
  dismissError: () => void;
  loading: boolean;
  invalidCredential: boolean;
  pendingAction: string | null;
  start: () => Promise<void>;
  /** Host only, before Start: choose the map (every call re-rolls the open slots). */
  chooseMap: (choice: MapChoice, startPreset?: string) => Promise<void>;
  join: (nickname: string, playerId?: string) => Promise<string | undefined>;
  leave: () => Promise<boolean>;
  removeBot: (playerId: string) => Promise<boolean>;
  /** Host only, before Start: seat random bots (no password). `fill` takes every open seat. */
  addRandomBots: (options?: {
    fill?: boolean;
    count?: number;
    nickname?: string;
    kind?: InProcessBotKind;
    faction?: string;
  }) => Promise<boolean>;
  /** Host only, before Start: choose (or, with `null`, clear) the faction of a bot seat. */
  setSeatFaction: (playerId: string, faction: string | null) => Promise<boolean>;
}

/** A single dropped background poll is noise; the indicator shows after this many in a row. */
export const LOBBY_POLL_FAILURE_LIMIT = 3;
/** A poll that has not answered after this long is dead (silent tunnel); the next tick starts afresh. */
export const LOBBY_POLL_TIMEOUT_MS = 10_000;

export function useLobbySession(gameId: string, playerSession?: string): LobbySessionState {
  const [lobby, setLobby] = useState<LobbyDto | null>(null);
  const [playerId, setPlayerId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pollFailures, setPollFailures] = useState(0);
  const [loading, setLoading] = useState(true);
  const [gone, setGone] = useState(false);
  const pollNow = useRef<() => void>(() => undefined);
  const [invalidCredential, setInvalidCredential] = useState(false);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const pending = useRef(false);
  const revision = useRef(0);
  /** A call of the lobby as the player of this page. */
  const post = (route: string, body?: object) =>
    lobbyPost(gameId, route, body, { session: playerSession });

  useEffect(() => {
    let active = true;
    // A slow server must not stack one more poll every tick (the browser runs out of sockets).
    let polling = false;
    let abortPoll: (() => void) | null = null;
    let generation = 0;
    setInvalidCredential(false);
    const load = async () => {
      if (pending.current || polling) return;
      polling = true;
      const mine = ++generation;
      const observed = revision.current;
      const controller = new AbortController();
      const deadline = window.setTimeout(() => controller.abort(), LOBBY_POLL_TIMEOUT_MS);
      abortPoll = () => controller.abort();
      try {
        // The public lobby intentionally has no viewer field. Reconnect explicitly to
        // obtain the authenticated identity; spectators only make a read-only GET.
        const response = await (playerSession
          ? joinLobby(gameId, { kind: "new" }, { session: playerSession, signal: controller.signal })
          : fetch(lobbyPath(gameId), { signal: controller.signal }));
        if (isGone(response) && active && mine === generation && !pending.current) setGone(true);
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
        if (active && mine === generation && observed === revision.current && !pending.current) {
          setLobby(next);
          setPlayerId(joined?.player.id ?? null);
          setLoading(false);
          setPollFailures(0);
          setGone(false);
        }
      } catch {
        // A background poll failure is not an action error: count it, show it only when it persists.
        if (active && mine === generation && observed === revision.current && !pending.current) {
          setPollFailures((count) => {
            // Until the first answer, "loading" lasts through the first few tries (the first
            // fetches after a long sleep often fail while the tunnel comes back).
            if (count + 1 >= LOBBY_POLL_FAILURE_LIMIT) setLoading(false);
            return count + 1;
          });
        }
      } finally {
        window.clearTimeout(deadline);
        if (mine === generation) {
          abortPoll = null;
          polling = false;
        }
      }
    };
    // Back from a long sleep: the poll in flight (if any) rides a dead connection; replace it.
    pollNow.current = () => {
      abortPoll?.();
      polling = false;
      void load();
    };
    void load();
    const timer = window.setInterval(() => void load(), 2_000);
    const stopResume = onResume(() => pollNow.current());
    return () => {
      active = false;
      window.clearInterval(timer);
      stopResume();
      abortPoll?.();
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
        setError(describeError(cause));
        return fallback;
      } finally {
        pending.current = false;
        setPendingAction(null);
      }
    },
    [],
  );

  const mutate = useCallback(
    async (path: "start" | "map", body?: object) =>
      run(
        path,
        async () => {
          const response = await post(path, body);
          if (!response.ok) {
            throw new Error(`Lobby ${path} failed (${response.status}): ${await response.text()}`);
          }
          setLobby(decodeLobby(await response.json(), gameId));
          setError(null);
        },
        undefined,
      ),
    [gameId, playerSession, run],
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
          const response = await joinLobby(
            gameId,
            takeoverId
              ? { kind: "takeover", player_id: takeoverId, nickname }
              : { kind: "new", nickname },
          );
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
    [gameId, run],
  );

  const leave = useCallback(async (): Promise<boolean> => {
    if (!playerSession) return false;
    return run(
      "leave",
      async () => {
        const response = await post("leave");
        if (!response.ok)
          throw new Error(`Leave lobby failed (${response.status}): ${await response.text()}`);
        // Validate the successful server response before removing the only local credential.
        decodeLobby(await response.json(), gameId);
        setError(null);
        return true;
      },
      false,
    );
  }, [gameId, playerSession, run]);

  const addRandomBots = useCallback(
    async (
      options: {
        fill?: boolean;
        count?: number;
        nickname?: string;
        kind?: InProcessBotKind;
        faction?: string;
      } = {},
    ): Promise<boolean> => {
      if (!playerSession) return false;
      return run(
        "add_random_bots",
        async () => {
          const response = await post("bots", {
            kind: options.kind ?? "random",
            ...(options.fill ? { fill: true } : {}),
            ...(options.count !== undefined && !options.fill ? { count: options.count } : {}),
            ...(options.nickname ? { nickname: options.nickname } : {}),
            ...(options.faction ? { faction: options.faction } : {}),
          });
          if (!response.ok) {
            const msg = await response.text();
            throw new Error(`Add ${options.kind ?? "random"} bot failed (${response.status}): ${msg}`);
          }
          const updated = decodeLobby(await response.json(), gameId);
          setLobby(updated);
          setError(null);
          return true;
        },
        false,
      );
    },
    [gameId, playerSession, run],
  );

  const setSeatFaction = useCallback(
    async (targetPlayerId: string, faction: string | null): Promise<boolean> => {
      if (!playerSession) return false;
      return run(
        "seat_faction",
        async () => {
          const response = await post("seat-faction", { player_id: targetPlayerId, faction });
          if (!response.ok) {
            const msg = await response.text();
            throw new Error(`Choose faction failed (${response.status}): ${msg}`);
          }
          const updated = decodeLobby(await response.json(), gameId);
          setLobby(updated);
          setError(null);
          return true;
        },
        false,
      );
    },
    [gameId, playerSession, run],
  );

  const removeBot = useCallback(
    async (targetPlayerId: string): Promise<boolean> => {
      if (!playerSession) return false;
      return run(
        "remove_bot",
        async () => {
          const response = await post("remove-bot", { player_id: targetPlayerId });
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
    [gameId, playerSession, run],
  );

  return {
    lobby,
    playerId,
    error,
    connectionLost: pollFailures >= LOBBY_POLL_FAILURE_LIMIT,
    gone,
    retry: () => {
      setLoading(true);
      setPollFailures(0);
      pollNow.current();
    },
    dismissError: () => setError(null),
    loading,
    invalidCredential,
    pendingAction,
    start: () => mutate("start"),
    chooseMap: (map, startPreset) =>
      mutate("map", startPreset === undefined ? { map } : { map, start_preset: startPreset }),
    join,
    leave,
    removeBot,
    addRandomBots,
    setSeatFaction,
  };
}
