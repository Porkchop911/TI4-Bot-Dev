import React from "react";
import { useSecondaryPrepMode } from "../hooks/useSecondaryPrepMode.ts";
import "./SecondaryPrep.css";

/**
 * Viewer-local setting for prepared strategy-card secondaries: review a prepared answer before it
 * is played (the default), or play it automatically when it is your turn to decide.
 */
export const SecondaryPrepModeToggle: React.FC = () => {
  const { mode, setMode } = useSecondaryPrepMode();
  return (
    <span
      className="secondary-prep-mode"
      data-testid="secondary-prep-mode"
      data-mode={mode}
      role="group"
      aria-label="Prepared secondary"
      title="What happens when the window opens for a secondary you prepared. Review: it is filled in and you confirm with one click. Auto: it is sent for you after a short delay (cancellable), only while it still validates."
    >
      <button
        type="button"
        data-testid="secondary-prep-mode-review"
        aria-pressed={mode === "review"}
        onClick={() => setMode("review")}
      >
        📝 Review
      </button>
      <button
        type="button"
        data-testid="secondary-prep-mode-auto"
        aria-pressed={mode === "auto"}
        onClick={() => setMode("auto")}
      >
        ⏩ Auto
      </button>
    </span>
  );
};
