import React, { useState } from "react";
import { ageLabel, forgetMyGame, tokenFor } from "../protocol/myGames.ts";

export interface OpenGameRow {
  game_id: string;
  phase?: "lobby" | "started";
  players?: number;
  created_at?: number | null;
}

interface Props {
  /** "limit": create was refused (429); "browse": opened from the start screen. */
  mode: "limit" | "browse";
  games: OpenGameRow[];
  onOpen: (gameId: string, token?: string) => void;
  /** The original create request again (limit mode). */
  onRetryCreate?: () => void;
  onClose: () => void;
  nowMs?: number;
}

const shortId = (id: string) => id.replace(/^game_/, "").slice(0, 6);

/** Lists the caller's open games with Open / Terminate; quick and plain by design. */
export const MyGamesDialog: React.FC<Props> = ({
  mode,
  games: initial,
  onOpen,
  onRetryCreate,
  onClose,
  nowMs,
}) => {
  const [games, setGames] = useState(initial);
  const [confirming, setConfirming] = useState<string | "all" | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ended, setEnded] = useState(0);

  const end = async (gameId: string): Promise<boolean> => {
    const token = tokenFor(gameId);
    if (!token) return false;
    try {
      const response = await fetch(`/api/games/${encodeURIComponent(gameId)}/terminate`, {
        method: "POST",
        headers: { "x-ti4-player-session": token },
      });
      // 404: already gone, which is what was wanted.
      if (response.ok || response.status === 404) {
        forgetMyGame(gameId);
        setGames((current) => current.filter((game) => game.game_id !== gameId));
        setEnded((count) => count + 1);
        return true;
      }
      setError(
        response.status === 403
          ? "Only the host of a game can end it."
          : `Could not end the game (${response.status}).`,
      );
    } catch {
      setError("Could not reach the server. Try again.");
    }
    return false;
  };

  const confirm = async () => {
    const target = confirming;
    if (!target) return;
    setBusy(true);
    setError(null);
    if (target === "all") {
      for (const game of games) if (tokenFor(game.game_id)) await end(game.game_id);
    } else await end(target);
    setConfirming(null);
    setBusy(false);
  };

  const mine = games.filter((game) => tokenFor(game.game_id));
  return (
    <div className="mygames-backdrop" role="presentation">
      <section
        className="panel mygames"
        role="dialog"
        aria-modal="true"
        aria-labelledby="mygames-title"
        data-testid="mygames-dialog"
      >
        <h2 id="mygames-title">
          {mode === "limit" ? "You have too many unfinished games" : "My games"}
        </h2>
        {mode === "limit" && (
          <p className="text-muted">
            End one or more of these to make room, then create your game.
          </p>
        )}
        {games.length === 0 && (
          <p className="text-muted" data-testid="mygames-empty">
            {mode === "limit" ? "No unfinished games left." : "No open games on this server."}
          </p>
        )}
        <ul className="mygames__list">
          {games.map((game) => {
            const token = tokenFor(game.game_id);
            const age = ageLabel(game.created_at, nowMs);
            const asking = confirming === game.game_id;
            return (
              <li key={game.game_id} className="mygames__row" data-testid="mygames-row">
                <div className="mygames__info">
                  <strong>Game {shortId(game.game_id)}</strong>
                  <span className="text-muted">
                    {game.phase === "started" ? "In progress" : game.phase === "lobby" ? "Lobby" : "Open"}
                    {game.players !== undefined &&
                      ` · ${game.players} player${game.players === 1 ? "" : "s"}`}
                    {age && ` · ${age}`}
                  </span>
                  {!token && (
                    <span className="text-faint" data-testid="mygames-not-yours">
                      Not yours (other browser)
                    </span>
                  )}
                </div>
                <div className="mygames__actions">
                  {asking ? (
                    <>
                      <span className="mygames__ask">Really end this game?</span>
                      <button
                        className="button button--danger"
                        disabled={busy}
                        onClick={() => void confirm()}
                      >
                        Yes
                      </button>
                      <button
                        className="button button--secondary"
                        disabled={busy}
                        onClick={() => setConfirming(null)}
                      >
                        No
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        className="button button--outline"
                        onClick={() => onOpen(game.game_id, token)}
                      >
                        Open
                      </button>
                      {token && (
                        <button
                          className="button button--danger-outline"
                          disabled={busy}
                          onClick={() => setConfirming(game.game_id)}
                        >
                          Terminate
                        </button>
                      )}
                    </>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
        {error && (
          <p className="mygames__error" role="alert">
            {error}
          </p>
        )}
        <div className="mygames__footer">
          {mine.length > 1 &&
            (confirming === "all" ? (
              <>
                <span className="mygames__ask">Really end all {mine.length} games?</span>
                <button className="button button--danger" disabled={busy} onClick={() => void confirm()}>
                  Yes
                </button>
                <button className="button button--secondary" onClick={() => setConfirming(null)}>
                  No
                </button>
              </>
            ) : (
              <button
                className="button button--danger-outline"
                disabled={busy}
                onClick={() => setConfirming("all")}
              >
                Terminate all my games
              </button>
            ))}
          {mode === "limit" && onRetryCreate && (
            <button
              className="button button--success"
              disabled={busy || ended === 0}
              onClick={onRetryCreate}
            >
              Create game
            </button>
          )}
          <button className="button button--secondary" onClick={onClose}>
            Close
          </button>
        </div>
      </section>
    </div>
  );
};
