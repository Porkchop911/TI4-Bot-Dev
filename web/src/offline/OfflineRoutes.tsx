import React, { useEffect, useState } from "react";
import { OfflineHome } from "./OfflineHome.tsx";
import { OfflineGameRoute } from "./OfflineGameRoute.tsx";
import { useOfflineLauncher } from "./useOfflineLauncher.ts";
import type { OfflineGameHandle } from "./types.ts";

export const isOfflinePath = (pathname: string): boolean => /^\/offline(\/|$)/.test(pathname);
const gameIdOf = (pathname: string): string | null => {
  const m = /^\/offline\/games\/([^/]+)$/.exec(pathname);
  return m ? decodeURIComponent(m[1]) : null;
};

/** Routes under /offline: the home screen and a running or resuming game. */
export const OfflineRoutes: React.FC<{
  onExit: () => void;
  renderGame?: (handle: OfflineGameHandle, banner: React.ReactNode) => React.ReactNode;
}> = ({ onExit, renderGame }) => {
  const launcher = useOfflineLauncher();
  const [path, setPath] = useState(() => window.location.pathname);
  const [entered, setEntered] = useState<OfflineGameHandle | null>(null);
  useEffect(() => {
    const receive = () => setPath(window.location.pathname);
    window.addEventListener("popstate", receive);
    return () => window.removeEventListener("popstate", receive);
  }, []);
  const go = (to: string) => {
    history.pushState({}, "", to);
    setPath(to);
  };
  if (!launcher)
    return (
      <main className="lobby-page" data-testid="offline-unavailable">
        <section className="panel lobby-panel">
          <h1>Offline play is not available</h1>
          <p className="text-muted">This build has no offline engine.</p>
          <button type="button" className="button button--outline" onClick={onExit}>
            Back to the start screen
          </button>
        </section>
      </main>
    );
  const gameId = gameIdOf(path);
  if (gameId)
    return (
      <OfflineGameRoute
        key={gameId}
        launcher={launcher}
        gameId={gameId}
        handle={entered?.gameId === gameId ? entered : null}
        renderGame={renderGame}
        onLeave={() => go("/offline")}
      />
    );
  return (
    <OfflineHome
      launcher={launcher}
      onBack={onExit}
      onEnter={(handle) => {
        setEntered(handle);
        go(`/offline/games/${encodeURIComponent(handle.gameId)}`);
      }}
    />
  );
};
