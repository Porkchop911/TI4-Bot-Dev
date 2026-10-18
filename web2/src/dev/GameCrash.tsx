import { type ReactNode, useState } from "react";
import { storeCrash } from "../session/savedGame";
import { exportSave, saveStore } from "../session/saves";
import { Button } from "../ui";
import { type Crash, CrashBoundary } from "./CrashBoundary";
import { deliverFile } from "./deliverFile";

/**
 * A local game that cannot fail to a blank page: an error of a render shows the error, with the
 * export of the saved game, which carries the error too. `locate` says the id of the game in the
 * save store, or null when the store has none.
 */
export function GameCrash({
  locate,
  children,
}: {
  locate: () => Promise<string | null>;
  children: ReactNode;
}) {
  return (
    <CrashBoundary
      onCrash={({ error, componentStack }) => {
        void locate().then((id) => {
          if (id !== null) {
            storeCrash(localStorage, id, {
              at: Date.now(),
              source: "render",
              message: error.message,
              ...(error.stack ? { stack: error.stack } : {}),
              componentStack,
            });
          }
        });
      }}
      fallback={(crash) => <CrashScreen crash={crash} locate={locate} />}
    >
      {children}
    </CrashBoundary>
  );
}

function CrashScreen({ crash, locate }: { crash: Crash; locate: () => Promise<string | null> }) {
  const [failure, setFailure] = useState<string | null>(null);
  const exportGame = async () => {
    setFailure(null);
    try {
      const id = await locate();
      if (id === null) {
        setFailure("This browser has no saved game of this address.");
        return;
      }
      const file = await exportSave(await saveStore(), localStorage, id);
      await deliverFile(file.filename, file.blob);
    } catch {
      setFailure("The game is not saved yet.");
    }
  };
  return (
    <main role="alert" className="flex h-dvh flex-col gap-3 overflow-y-auto p-5">
      <p className="text-red">The game stopped with an error: {crash.error.message}</p>
      <p className="text-muted">The export has the saved game and this error.</p>
      <span className="flex gap-2">
        <Button onClick={exportGame}>Export</Button>
        <Button tone="quiet" onClick={() => location.assign("/")}>
          Start
        </Button>
      </span>
      {failure && <p className="text-red">{failure}</p>}
      <pre className="text-xs break-all whitespace-pre-wrap text-muted">
        {crash.error.stack}
        {crash.componentStack}
      </pre>
    </main>
  );
}
