import React, { useCallback, useRef, useState } from "react";
import { shortEngine } from "./engineInfo.ts";
import type { OfflineMismatchChoice, OfflineMismatchPrompt } from "./types.ts";

const DIFFERS: Record<OfflineMismatchPrompt["differs"][number], string> = {
  build: "engine version",
  contentDigest: "game data",
  wasmSha256: "engine file",
};

/**
 * "This game was saved with another version of the engine": view only, try anyway, or delete.
 * Shown before the replay (engine changed) and after it (the replay gave other decisions).
 * Nothing is replayed, played or deleted until a button is pressed.
 */
export const EngineMismatchDialog: React.FC<{
  prompt: OfflineMismatchPrompt;
  onChoose: (choice: OfflineMismatchChoice) => void;
}> = ({ prompt, onChoose }) => {
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  return (
    <section
      className="panel offline-mismatch"
      role="alertdialog"
      aria-labelledby="offline-mismatch-title"
      aria-describedby="offline-mismatch-text"
      data-testid="offline-mismatch"
      data-reason={prompt.reason}
    >
      <h2 id="offline-mismatch-title">{prompt.title}</h2>
      <p id="offline-mismatch-text" className="text-muted">
        {prompt.message}
      </p>
      <dl className="offline-mismatch__engines" data-testid="offline-mismatch-engines">
        <dt>Saved with</dt>
        <dd>
          engine {shortEngine(prompt.saved.wasmSha256)}, version {prompt.saved.build}
        </dd>
        <dt>This page</dt>
        <dd>
          engine {shortEngine(prompt.current.wasmSha256)}, version {prompt.current.build}
        </dd>
        {prompt.differs.length > 0 && (
          <>
            <dt>Differs in</dt>
            <dd>{prompt.differs.map((d) => DIFFERS[d]).join(", ")}</dd>
          </>
        )}
      </dl>
      <div className="offline-mismatch__actions">
        {confirmingDelete ? (
          <>
            <span className="text-muted">Delete this game for good?</span>
            <button
              type="button"
              className="button button--danger"
              data-testid="offline-mismatch-delete-confirm"
              onClick={() => onChoose("delete")}
            >
              Delete
            </button>
            <button type="button" className="button button--outline" onClick={() => setConfirmingDelete(false)}>
              Keep
            </button>
          </>
        ) : (
          prompt.choices.map((choice) => (
            <button
              key={choice.id}
              type="button"
              className={`button ${choice.destructive ? "button--danger" : choice.id === "view-only" ? "button--primary" : "button--outline"}`}
              data-testid={`offline-mismatch-${choice.id}`}
              onClick={() => (choice.destructive ? setConfirmingDelete(true) : onChoose(choice.id))}
            >
              {choice.label}
            </button>
          ))
        )}
      </div>
    </section>
  );
};

/** State for the dialog: `decide` goes to `launcher.resume`, `dialog` is rendered by the screen. */
export function useMismatchDecider(): {
  decide: (prompt: OfflineMismatchPrompt) => Promise<OfflineMismatchChoice>;
  dialog: React.ReactNode;
  pending: boolean;
} {
  const [prompt, setPrompt] = useState<OfflineMismatchPrompt | null>(null);
  const resolver = useRef<((choice: OfflineMismatchChoice) => void) | null>(null);
  const decide = useCallback(
    (next: OfflineMismatchPrompt) =>
      new Promise<OfflineMismatchChoice>((resolve) => {
        resolver.current = resolve;
        setPrompt(next);
      }),
    [],
  );
  const dialog = prompt ? (
    <EngineMismatchDialog
      prompt={prompt}
      onChoose={(choice) => {
        resolver.current?.(choice);
        resolver.current = null;
        setPrompt(null);
      }}
    />
  ) : null;
  return { decide, dialog, pending: prompt !== null };
}
