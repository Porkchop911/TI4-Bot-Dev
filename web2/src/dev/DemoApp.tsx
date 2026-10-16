import { useEffect, useMemo } from "react";
import { GameShell } from "../game/shell/GameShell";
import { DemoBar } from "../mock/DemoBar";
import { createMockStore, useMockSession } from "../mock/store";
import { LOCAL_GAME } from "./defaultGame";

/**
 * The game shell on the mock session. `?example=draft-movement&viewer=hacan` opens one state directly.
 * `notice` says why the page is the demo and not the local game (the engine did not load).
 */
export function DemoApp({ notice }: { notice?: string }) {
  const store = useMemo(() => {
    const params = new URLSearchParams(location.search);
    return createMockStore({
      example: params.get("example") ?? undefined,
      viewer: params.get("viewer") ?? undefined,
    });
  }, []);
  const { session, demo } = useMockSession(store);
  useEffect(() => {
    const params = new URLSearchParams(location.search);
    params.set("example", demo.example);
    if (demo.viewer === "sol") {
      params.delete("viewer");
    } else {
      params.set("viewer", demo.viewer);
    }
    history.replaceState(null, "", `?${params}`);
  }, [demo.example, demo.viewer]);
  return (
    <div className="flex h-dvh flex-col [--inset-bottom:0px]">
      {notice && (
        <p
          role="status"
          className="shrink-0 border-b border-line bg-surface px-4 py-1.5 text-xs text-muted"
        >
          {notice}
        </p>
      )}
      {/* The demo bar is under the shell, so it has the bottom edge of the screen, not the shell. */}
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)] grid-rows-[minmax(0,1fr)_auto]">
        <GameShell session={session} other={{ label: "Local game", href: LOCAL_GAME }} />
        <DemoBar demo={demo} />
      </div>
    </div>
  );
}
