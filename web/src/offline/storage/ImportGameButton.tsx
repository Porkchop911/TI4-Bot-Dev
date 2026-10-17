import { useRef, useState } from "react";
import { StoreError } from "@ti4/shared/storage/index.ts";

/**
 * "Import a game" button: a file picker that hands the chosen JSON file to `store.importGame`.
 * Sits next to "My offline games". A problem with the file is shown under the button.
 */
export function ImportGameButton(props: {
  /** `OfflineSaveStore.importGame`, or the launcher's `storage.importGame`. */
  importGame: (file: Blob) => Promise<{ id: string; engineDiffers: boolean }>;
  onImported: (outcome: { id: string; engineDiffers: boolean }) => void;
  /** Where a refused file's reason goes; without it the reason is shown under the button. */
  onProblem?: (message: string | null) => void;
  disabled?: boolean;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [problem, setProblem] = useState<string | null>(null);
  return (
    <>
      <button
        type="button"
        className="button button--outline"
        data-testid="offline-import"
        disabled={props.disabled}
        onClick={() => input.current?.click()}
      >
        Import a game
      </button>
      <input
        ref={input}
        type="file"
        accept="application/json,.json"
        hidden
        data-testid="import-game-file"
        onChange={async (e) => {
          const file = e.currentTarget.files?.[0];
          e.currentTarget.value = "";
          if (!file) return;
          setProblem(null);
          props.onProblem?.(null);
          try {
            props.onImported(await props.importGame(file));
          } catch (err) {
            const message = err instanceof StoreError ? err.message : "The file could not be imported.";
            if (props.onProblem) props.onProblem(message);
            else setProblem(message);
          }
        }}
      />
      {problem && (
        <p role="alert" className="session-error offline-error" data-testid="offline-import-problem">
          {problem}
        </p>
      )}
    </>
  );
}
