import React, { useState } from "react";
import { ageLabel } from "../protocol/myGames.ts";
import { lobbyFactionName } from "../presentation/lobbyFactions.ts";
import { durationLabel } from "./setup.ts";
import { NOT_PLAYABLE_HERE } from "./storage/shell.ts";
import type { OfflineGameSummary } from "./types.ts";

/** The factions of a game, seat by seat; the seats of people are marked. Null when none is known. */
function seatsLine(game: OfflineGameSummary): string | null {
  if (!game.factions.some((faction) => faction !== null)) return null;
  return game.factions
    .map((faction, i) => {
      const name = faction ? lobbyFactionName(faction) : "?";
      return game.humanSeats.includes(i + 1) ? `${name} (you)` : name;
    })
    .join(", ");
}

/** "My offline games": resume, export (backup file) and delete with a confirmation. */
export const OfflineGamesList: React.FC<{
  games: OfflineGameSummary[];
  onResume: (gameId: string) => void;
  onDelete: (gameId: string) => void | Promise<void>;
  onExport: (gameId: string) => void | Promise<void>;
  disabled?: boolean;
  nowMs?: number;
}> = ({ games, onResume, onDelete, onExport, disabled, nowMs }) => {
  const [confirming, setConfirming] = useState<string | null>(null);
  if (games.length === 0)
    return (
      <p className="text-muted" data-testid="offline-games-empty">
        No offline games on this device yet.
      </p>
    );
  return (
    <ul className="mygames__list offline-games" data-testid="offline-games">
      {games.map((game) => {
        const asking = confirming === game.gameId;
        const age = ageLabel(game.updatedAt, nowMs);
        const seats = seatsLine(game);
        return (
          <li key={game.gameId} className="mygames__row offline-games__row" data-testid="offline-game-row">
            <div className="mygames__info">
              <strong>
                {game.humanFaction ? lobbyFactionName(game.humanFaction) : "Faction pending"}
                {" · "}
                {game.playerCount} players
              </strong>
              <span className="text-muted">
                {game.status === "finished"
                  ? "Finished"
                  : game.round !== null
                    ? `Round ${game.round}${game.phase ? `, ${game.phase}` : ""}`
                    : "Not started"}
                {game.humanSeats.length === 1 ? ` · seat ${game.humanSeat}` : ` · seats ${game.humanSeats.join(", ")}`}
                {age && ` · ${age}`}
              </span>
              {seats && (
                <span className="text-muted" data-testid="offline-game-factions">
                  {seats}
                </span>
              )}
              {!game.playable ? (
                <span className="text-faint" data-testid="offline-game-not-playable">
                  {NOT_PLAYABLE_HERE}
                </span>
              ) : game.status === "in_progress" && (
                <span className="text-faint">Restoring takes {durationLabel(game.resumeEstimateMs)}.</span>
              )}
            </div>
            <div className="mygames__actions">
              {asking ? (
                <>
                  <span className="text-muted">Delete this game?</span>
                  <button
                    type="button"
                    className="button button--danger"
                    data-testid="offline-delete-confirm"
                    disabled={disabled}
                    onClick={async () => {
                      await onDelete(game.gameId);
                      setConfirming(null);
                    }}
                  >
                    Delete
                  </button>
                  <button type="button" className="button button--outline" onClick={() => setConfirming(null)}>
                    Keep
                  </button>
                </>
              ) : (
                <>
                  {game.playable && (
                    <button
                      type="button"
                      className="button button--primary"
                      data-testid="offline-resume"
                      disabled={disabled}
                      onClick={() => onResume(game.gameId)}
                    >
                      {game.status === "finished" ? "View" : "Resume"}
                    </button>
                  )}
                  <button
                    type="button"
                    className="button button--outline"
                    data-testid="offline-export"
                    disabled={disabled}
                    onClick={() => void onExport(game.gameId)}
                  >
                    Export
                  </button>
                  <button
                    type="button"
                    className="button button--outline"
                    data-testid="offline-delete"
                    disabled={disabled}
                    onClick={() => setConfirming(game.gameId)}
                  >
                    Delete
                  </button>
                </>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
};

/** The saved games of an older version: not read, only deleted (with a confirmation). */
export const OlderSaves: React.FC<{ count: number; onDelete: () => void | Promise<void>; disabled?: boolean }> = ({
  count,
  onDelete,
  disabled,
}) => {
  const [asking, setAsking] = useState(false);
  if (count === 0) return null;
  return (
    <div className="mygames__row offline-games__row" data-testid="offline-older-saves">
      <div className="mygames__info">
        <strong>
          {count === 1 ? "1 saved game" : `${count} saved games`} from an older version
        </strong>
        <span className="text-muted">This version cannot open {count === 1 ? "it" : "them"}.</span>
      </div>
      <div className="mygames__actions">
        {asking ? (
          <>
            <span className="text-muted">Delete {count === 1 ? "it" : "them"}?</span>
            <button
              type="button"
              className="button button--danger"
              data-testid="offline-older-delete-confirm"
              disabled={disabled}
              onClick={async () => {
                await onDelete();
                setAsking(false);
              }}
            >
              Delete
            </button>
            <button type="button" className="button button--outline" onClick={() => setAsking(false)}>
              Keep
            </button>
          </>
        ) : (
          <button
            type="button"
            className="button button--outline"
            data-testid="offline-older-delete"
            disabled={disabled}
            onClick={() => setAsking(true)}
          >
            Delete
          </button>
        )}
      </div>
    </div>
  );
};
