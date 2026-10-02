import React, { useEffect, useMemo, useState } from "react";
import { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { SemanticIntent, usePipelineRunner } from "../hooks/usePipelineRunner.ts";
import { DecisionHeader } from "./DecisionHeader.tsx";

export const CargoLoadingTray: React.FC<{
  choice: PendingChoiceDto;
  board?: BoardView;
  onSubmit: (id: string) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
}> = ({ choice, board, onSubmit, isOpen, onClose, lastError }) => {
  const [staged, setStaged] = useState<Record<string, number>>({});
  const [submittingDone, setSubmittingDone] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const {
    executePipeline,
    isRunning,
    lastError: pipelineError,
  } = usePipelineRunner(choice, onSubmit);
  const done = choice.options.find(
    (option) => option.id === "done_loading" || option.kind === "decline",
  );
  const capacityFree = Number(
    choice.options.find((option) => typeof option.payload?.capacity_remaining === "number")?.payload
      ?.capacity_remaining ?? 0,
  );
  const loadedGround = Number(done?.payload?.loaded_ground ?? 0);
  const loadedFighters = Number(done?.payload?.loaded_fighters ?? 0);
  const groups = useMemo(
    () =>
      choice.options
        .filter((option) => option.kind !== "decline" && option.id !== "done_loading")
        .map((option) => {
          const unit = String(option.payload?.unit ?? option.label);
          const source = typeof option.payload?.source === "string" ? option.payload.source : null;
          const damaged = option.payload?.damaged === true;
          const key = JSON.stringify([unit, source, damaged, option.payload?.galvanized === true]);
          const origin =
            choice.context?.target && "System" in choice.context.target
              ? choice.context.target.System
              : String(option.payload?.system ?? "");
          const onBoard =
            board?.systems?.[origin]?.units.filter(
              (u) =>
                u.owner === choice.actor &&
                u.unit_type === unit &&
                (u.planet ?? null) === source &&
                Boolean(u.damaged) === damaged,
            ).length ?? 0;
          return { key, unit, source, damaged, option, onBoard };
        }),
    [choice, board],
  );
  const groupAvailable = (group: (typeof groups)[number]) => {
    const isFighter = group.unit.toLowerCase().includes("fighter");
    const isGround =
      group.unit.toLowerCase().includes("infantry") || group.unit.toLowerCase().includes("mech");
    const peers = groups.filter((other) =>
      isFighter
        ? other.unit.toLowerCase().includes("fighter")
        : isGround
          ? /infantry|mech/i.test(other.unit)
          : other.key === group.key,
    );
    const alreadyLoaded =
      peers.length === 1 ? (isFighter ? loadedFighters : isGround ? loadedGround : 0) : 0;
    const groundAvailable = isGround
      ? Number(done?.payload?.ground_available ?? Infinity)
      : Infinity;
    return Math.max(
      1,
      Math.min(groundAvailable, group.onBoard ? group.onBoard - alreadyLoaded : 1),
    );
  };
  const stagedCount = Object.values(staged).reduce((sum, count) => sum + count, 0);
  useEffect(() => {
    setStaged({});
    setLocalError(null);
    setSubmittingDone(false);
  }, [choice.nonce]);

  const submit = async () => {
    setLocalError(null);
    if (isRunning || submittingDone) return;
    if (stagedCount === 0) {
      if (!done) {
        setLocalError("No finish-loading option was offered.");
        return;
      }
      setSubmittingDone(true);
      try {
        await onSubmit(done.id);
      } catch (error) {
        setLocalError(String(error));
      } finally {
        setSubmittingDone(false);
      }
      return;
    }
    const intents: SemanticIntent[] = groups.flatMap((group) =>
      Array.from({ length: staged[group.key] ?? 0 }, () => ({
        predicate: (option: ChoiceOptionDto) =>
          option.kind !== "decline" &&
          option.payload?.unit === group.unit &&
          (typeof option.payload?.source === "string" ? option.payload.source : null) ===
            group.source &&
          (option.payload?.damaged === true) === group.damaged &&
          (option.payload?.galvanized === true) === (group.option.payload?.galvanized === true),
      })),
    );
    // Full holds (or the last available unit) close automatically; there is no next "done" decision.
    if (
      done &&
      stagedCount < capacityFree &&
      groups.some((group) => (staged[group.key] ?? 0) < groupAvailable(group))
    ) {
      intents.push({ predicate: (option) => option.id === done.id });
    }
    executePipeline(intents);
  };

  if (!isOpen) return null;
  return (
    <aside
      className="cargo-tray panel"
      role="region"
      aria-label="Load carrier cargo"
      data-testid="cargo-loading-tray"
    >
      <DecisionHeader
        actor={choice.actor}
        title="Load units"
        instruction={choice.prompt}
        onMinimize={onClose}
      />
      <p className="workflow-copy">
        Choose units to carry. Adjust or reset your selection before confirming.
      </p>
      <div className="cargo-tray__summary" data-testid="cargo-summary">
        <span>Fighters loaded: {loadedFighters}</span>
        <span>Ground forces loaded: {loadedGround}</span>
        <span>Staged: {stagedCount}</span>
        <span>
          Free slots: {capacityFree - stagedCount} / {capacityFree}
        </span>
      </div>
      <div className="cargo-tray__list">
        {groups.map((group) => {
          const count = staged[group.key] ?? 0;
          const max = groupAvailable(group); // Server offers at least one candidate even if a board projection is absent.
          return (
            <div className="workflow-card cargo-tray__row" key={group.key}>
              <div>
                <strong>
                  {group.unit}
                  {group.damaged ? " (damaged)" : ""}
                </strong>
                <div className="text-muted">
                  From {group.source ?? "space"} · Have: {group.onBoard || "unknown"} · Available:{" "}
                  {max} · Staged: {count}
                </div>
              </div>
              <div className="workflow-row">
                <button
                  type="button"
                  className="button button--secondary button--icon"
                  aria-label={`Remove ${group.unit} from ${group.source ?? "space"}`}
                  disabled={count === 0 || isRunning}
                  onClick={() => setStaged((prev) => ({ ...prev, [group.key]: count - 1 }))}
                >
                  −
                </button>
                <span className="workflow-count">{count}</span>
                <button
                  type="button"
                  className="button button--secondary button--icon"
                  aria-label={`Stage ${group.unit} from ${group.source ?? "space"}`}
                  disabled={count >= max || stagedCount >= capacityFree || isRunning}
                  onClick={() => setStaged((prev) => ({ ...prev, [group.key]: count + 1 }))}
                >
                  +
                </button>
              </div>
            </div>
          );
        })}
      </div>
      {(lastError || localError || pipelineError) && (
        <div role="alert" className="workflow-error">
          {localError || pipelineError || lastError}
        </div>
      )}
      {stagedCount > 1 && (
        <p className="text-muted">
          Loads are submitted one decision at a time; later units must be offered again.
        </p>
      )}
      <div className="workflow-actions workflow-actions--split">
        <button
          type="button"
          className="button button--secondary"
          disabled={stagedCount === 0 || isRunning}
          onClick={() => setStaged({})}
        >
          Reset selection
        </button>
        <button
          type="button"
          className="button button--primary"
          disabled={isRunning || submittingDone}
          onClick={() => void submit()}
        >
          {isRunning || submittingDone
            ? "Loading…"
            : stagedCount
              ? `Confirm ${stagedCount} load${stagedCount === 1 ? "" : "s"}`
              : "Done loading"}
        </button>
      </div>
    </aside>
  );
};
