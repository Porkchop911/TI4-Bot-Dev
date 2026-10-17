import React, { useCallback, useEffect, useRef, useState } from "react";
import { OfflineSetupForm } from "./OfflineSetupForm.tsx";
import { OfflineGamesList, OlderSaves } from "./OfflineGamesList.tsx";
import { ResumeProgress } from "./ResumeProgress.tsx";
import { OfflineBanner } from "./OfflineBanner.tsx";
import { useMismatchDecider } from "./EngineMismatchDialog.tsx";
import { ImportGameButton } from "./storage/ImportGameButton.tsx";
import { CapabilityProvider } from "./capabilities.ts";
import {
  OfflineGameDeletedError,
  type OfflineGameHandle,
  type OfflineGameSummary,
  type OfflineLauncher,
  type OfflineProgress,
  type OfflineSetup,
} from "./types.ts";
import "./offline.css";

type Busy = { kind: "create" | "resume"; progress: OfflineProgress; estimateMs: number | null };

/**
 * /offline: the setup form and "My offline games". Starting or resuming shows the progress bar;
 * when the launcher returns a handle, `onEnter` navigates to the game (T5 renders it).
 */
export const OfflineHome: React.FC<{
  launcher: OfflineLauncher;
  onEnter: (handle: OfflineGameHandle) => void;
  onBack: () => void;
}> = ({ launcher, onEnter, onBack }) => {
  const [games, setGames] = useState<OfflineGameSummary[] | null>(null);
  /** Saved games of an older version (counted when the list is read). */
  const [older, setOlder] = useState(0);
  const [busy, setBusy] = useState<Busy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const abort = useRef<AbortController | null>(null);
  const mismatch = useMismatchDecider();
  const [notice, setNotice] = useState<string | null>(null);
  const support = launcher.supported();

  const refresh = useCallback(async () => {
    try {
      setGames(await launcher.list());
      setOlder(launcher.storage?.status().unreadable ?? 0);
    } catch {
      setGames([]);
      setError("Could not read the saved games on this device.");
    }
  }, [launcher]);
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = async (
    kind: Busy["kind"],
    estimateMs: number | null,
    start: (cb: (p: OfflineProgress) => void, signal: AbortSignal) => Promise<OfflineGameHandle>,
  ) => {
    const controller = new AbortController();
    abort.current = controller;
    setError(null);
    setNotice(null);
    setBusy({ kind, estimateMs, progress: { phase: "loading-engine", done: 0, total: null } });
    try {
      const handle = await start((progress) => setBusy({ kind, estimateMs, progress }), controller.signal);
      if (!controller.signal.aborted) onEnter(handle);
    } catch (cause) {
      if (cause instanceof OfflineGameDeletedError) setNotice("The game was deleted.");
      else if (!controller.signal.aborted)
        setError(cause instanceof Error ? cause.message : "The offline game could not be opened.");
    } finally {
      abort.current = null;
      setBusy(null);
      void refresh();
    }
  };

  const create = (setup: OfflineSetup) =>
    void run("create", null, (cb, signal) => launcher.create(setup, cb, signal));
  const resume = (gameId: string) =>
    void run("resume", games?.find((g) => g.gameId === gameId)?.resumeEstimateMs ?? null, (cb, signal) =>
      launcher.resume(gameId, cb, signal, { decide: mismatch.decide }),
    );
  const exportGame = async (gameId: string) => {
    try {
      const { filename, blob } = await launcher.exportGame(gameId);
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = filename;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch {
      setError("Could not export that game.");
    }
  };

  return (
    <CapabilityProvider value={launcher.capabilities}>
      <main data-testid="offline-home" className="lobby-page">
        <section className="panel lobby-panel offline-panel">
          <OfflineBanner capabilities={launcher.capabilities} feedback={launcher.feedback} storage={launcher.storage} />
          <h1>Play offline</h1>
          <p className="text-muted">
            One human against rule bots, on this device. No account or connection needed.
          </p>
          {!support.ok && (
            <div className="session-error offline-error" role="alert" data-testid="offline-unsupported">
              {support.reason}
            </div>
          )}
          {busy && mismatch.pending ? (
            mismatch.dialog
          ) : busy ? (
            <ResumeProgress
              progress={busy.progress}
              estimateMs={busy.estimateMs}
              onCancel={() => abort.current?.abort()}
            />
          ) : (
            <>
              {support.ok && <OfflineSetupForm factions={launcher.factions} maps={launcher.maps} onStart={create} />}
              <div className="offline-games-head">
                <h2 className="offline-heading">My offline games</h2>
                {launcher.storage && (
                  <ImportGameButton
                    importGame={(file) => launcher.storage!.importGame(file)}
                    disabled={!support.ok}
                    onProblem={(message) => {
                      setError(message);
                      if (message) setNotice(null);
                    }}
                    onImported={(outcome) => {
                      setError(null);
                      setNotice(
                        outcome.engineDiffers
                          ? "Game imported. It was saved with another engine: you will be asked what to do when you open it."
                          : "Game imported.",
                      );
                      void refresh();
                    }}
                  />
                )}
              </div>
              {games === null ? (
                <p className="text-muted">Loading…</p>
              ) : (
                <OfflineGamesList
                  games={games}
                  disabled={!support.ok}
                  onResume={resume}
                  onExport={exportGame}
                  onDelete={async (id) => {
                    await launcher.remove(id);
                    await refresh();
                  }}
                />
              )}
              {launcher.storage && (
                <OlderSaves
                  count={older}
                  onDelete={async () => {
                    await launcher.storage!.removeUnreadable();
                    await refresh();
                  }}
                />
              )}
            </>
          )}
          {notice && (
            <p className="text-muted" role="status" data-testid="offline-notice">
              {notice}
            </p>
          )}
          {error && (
            <div className="session-error offline-error" role="alert" data-testid="offline-error">
              {error}
            </div>
          )}
          <button type="button" className="button button--outline" data-testid="offline-back" onClick={onBack}>
            Back to the start screen
          </button>
        </section>
      </main>
    </CapabilityProvider>
  );
};
