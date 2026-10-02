import React, { useEffect, useMemo, useState } from "react";
import { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { Dialog } from "../primitives/index.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";

export interface ProductionBuilderDrawerProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
  queuedUnits?: readonly string[];
  onQueueProduction?: (units: string[]) => void;
  onSubmitBatch?: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
}

function draftResourceCost(options: ChoiceOptionDto[], draft: Record<string, number>): number {
  let printedTotal = 0;
  let discountOnce = 0;
  for (const option of options) {
    const batches = draft[option.id] ?? 0;
    if (!batches) continue;
    if (option.payload?.free_this_use === true) continue;
    const cost = option.payload?.cost;
    if (typeof cost !== "number" || !Number.isSafeInteger(cost) || cost < 0) return Infinity;
    const printed = option.payload?.printed_cost;
    const discount = option.payload?.discount;
    if (
      typeof printed === "number" &&
      Number.isSafeInteger(printed) &&
      printed >= cost &&
      typeof discount === "number" &&
      Number.isSafeInteger(discount) &&
      discount >= 0
    ) {
      printedTotal += batches * printed;
      // Every offered option previews the same one-time discount. Count it once, not per batch.
      discountOnce = Math.max(discountOnce, Math.min(discount, printed));
    } else {
      printedTotal += batches * cost;
    }
  }
  return Math.max(0, printedTotal - discountOnce);
}

export const ProductionBuilderDrawer: React.FC<ProductionBuilderDrawerProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isOpen,
  onClose,
  lastError,
  queuedUnits = [],
  onQueueProduction,
  onSubmitBatch,
}) => {
  const display = usePlayerIdentity();
  const [draft, setDraft] = useState<Record<string, number>>({});
  const [batchRunning, setBatchRunning] = useState(false);
  const [batchError, setBatchError] = useState<string | null>(null);
  useEffect(() => setDraft({}), [choice?.nonce]);
  const subtype = choice?.context?.subtype ?? "";
  const isPlaceUnit = subtype === "place_unit";
  const isProduceUnit = subtype === "produce_unit" || !isPlaceUnit;

  const productionOptions = useMemo(() => {
    if (!choice) return [];
    return choice.options.filter((o) => o.id !== "decline" && o.kind !== "decline");
  }, [choice]);

  const constraints = model?.outstanding?.[0] ?? choice?.context?.outstanding?.[0];
  const capacityLimit =
    model?.selectionMode.mode === "production"
      ? model.selectionMode.capacity
      : (constraints?.amount ?? 0);
  const capacitySpent = constraints?.paid ?? 0;
  const capacityRemaining = Math.max(0, capacityLimit - capacitySpent);
  const stagedBatches = productionOptions.reduce((sum, opt) => sum + (draft[opt.id] ?? 0), 0);
  const stagedCapacity = productionOptions.reduce(
    (sum, opt) =>
      sum +
      (draft[opt.id] ?? 0) *
        (typeof opt.payload?.production_spent === "number"
          ? opt.payload.production_spent
          : typeof opt.payload?.placed === "number"
            ? opt.payload.placed
            : typeof opt.payload?.count === "number"
              ? opt.payload.count
              : 1),
    0,
  );
  const stagedCost = draftResourceCost(productionOptions, draft);
  const availableResources = productionOptions.find(
    (opt) => typeof opt.payload?.available_resources === "number",
  )?.payload?.available_resources;
  const carriedCredit = productionOptions.find((opt) => typeof opt.payload?.credit === "number")
    ?.payload?.credit;
  const resourceLimit =
    typeof availableResources === "number" &&
    Number.isSafeInteger(availableResources) &&
    availableResources >= 0
      ? availableResources +
        (typeof carriedCredit === "number" &&
        Number.isSafeInteger(carriedCredit) &&
        carriedCredit > 0
          ? carriedCredit
          : 0)
      : null;
  const capacityUsed = capacitySpent + stagedCapacity;
  const queued = queuedUnits.length > 0;

  const systemId =
    model?.selectionMode.mode === "production"
      ? model.selectionMode.systemId
      : choice?.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";

  if (!isOpen || !choice) return null;

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Content className="choice-workflow-dialog production-dialog">
        <div data-testid="production-builder-drawer" className="production-drawer">
          <Dialog.Title as="h2" className="visually-hidden">
            {choice.prompt}
          </Dialog.Title>
          <DecisionHeader
            actor={choice.actor}
            title={isPlaceUnit ? "Choose a placement" : "Produce units"}
            instruction={choice.prompt}
            progress={systemId ? `System ${systemId}` : undefined}
            onMinimize={onClose}
            titleTestId="production-drawer-title"
            minimizeTestId="close-production-drawer"
          />

          <WorkflowShell
            choice={choice}
            model={model}
            viewerSeat={viewerSeat}
            onSubmit={onSubmit}
            lastError={batchError ?? lastError}
            spectatorNotice={`Observing unit production in progress for ${display(choice.actor).label}...`}
            spectatorNoticeTestId="spectator-production-notice"
            errorTestId="production-error-banner"
          >
            {({ isActor, isDirectSubmitting: isSubmitting, declineOption, submitDirect }) => (
              <>
                {/* Produce Unit Mode */}
                {isActor && isProduceUnit && (
                  <div className="workflow-stack">
                    {/* Capacity Progress Meter */}
                    {capacityLimit > 0 && (
                      <div className="workflow-card production-drawer__meter">
                        <div className="workflow-card--row">
                          <span className="text-muted">Production Capacity:</span>
                          <span data-testid="production-capacity-counter" className="text-success">
                            {capacityUsed} / {capacityLimit} Units (
                            {Math.max(0, capacityLimit - capacityUsed)} Left)
                          </span>
                        </div>

                        <progress
                          className="production-drawer__progress"
                          data-full={capacityUsed >= capacityLimit}
                          max={capacityLimit}
                          value={capacityUsed}
                        />
                      </div>
                    )}
                    <div className="workflow-card production-drawer__meter">
                      <div className="workflow-card--row">
                        <span className="text-muted">Resources:</span>
                        <span data-testid="production-resources-counter" className="text-success">
                          {resourceLimit === null
                            ? "Unavailable"
                            : `${stagedCost} / ${resourceLimit} Resources (${Math.max(0, resourceLimit - stagedCost)} Left)`}
                        </span>
                      </div>
                      {resourceLimit !== null && (
                        <progress
                          className="production-drawer__progress"
                          max={Math.max(1, resourceLimit)}
                          value={stagedCost}
                        />
                      )}
                    </div>

                    {/* Units Grid */}
                    <div data-testid="production-options-grid" className="production-drawer__grid">
                      {productionOptions.map((opt) => {
                        const capacity =
                          typeof opt.payload?.production_spent === "number"
                            ? opt.payload.production_spent
                            : typeof opt.payload?.placed === "number"
                              ? opt.payload.placed
                              : typeof opt.payload?.count === "number"
                                ? opt.payload.count
                                : 1;
                        const count = draft[opt.id] ?? 0;
                        const label = opt.label.replace(/^produce\s+/i, "");
                        const nextCost = draftResourceCost(productionOptions, {
                          ...draft,
                          [opt.id]: count + 1,
                        });
                        return (
                          <div key={opt.id} className="workflow-card production-drawer__unit">
                            <span>{label}</span>
                            <div className="workflow-row">
                              <button
                                type="button"
                                className="button button--secondary button--icon"
                                aria-label={`Remove ${label}`}
                                disabled={!count || queued || isSubmitting}
                                onClick={() =>
                                  setDraft((prev) => ({ ...prev, [opt.id]: count - 1 }))
                                }
                              >
                                −
                              </button>
                              <span
                                className="workflow-count"
                                data-testid={`produce-count-${opt.id}`}
                              >
                                {count}
                              </span>
                              <button
                                type="button"
                                className="button button--secondary button--icon"
                                data-testid={`produce-unit-btn-${opt.id}`}
                                aria-label={`Add ${label}`}
                                disabled={
                                  queued ||
                                  isSubmitting ||
                                  capacity < 0 ||
                                  stagedCapacity + capacity > capacityRemaining ||
                                  resourceLimit === null ||
                                  nextCost > resourceLimit
                                }
                                onClick={() =>
                                  setDraft((prev) => ({ ...prev, [opt.id]: count + 1 }))
                                }
                              >
                                +
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>

                    <div className="production-drawer__footer">
                      {stagedBatches > 1 && (
                        <p className="text-muted">
                          Staged builds submit one decision at a time. The queue pauses for payment
                          or placement and stops if a later offer changes.
                        </p>
                      )}
                      <button
                        type="button"
                        className="button button--secondary"
                        disabled={!stagedBatches || queued || isSubmitting}
                        onClick={() => setDraft({})}
                      >
                        Reset selection
                      </button>
                      <button
                        type="button"
                        className="button button--primary"
                        disabled={!stagedBatches || queued || isSubmitting || batchRunning}
                        onClick={async () => {
                          // A build can open a payment or placement decision before
                          // another build is offered. The shell's queue waits for that choice
                          // to resolve instead of sending an invalid produce-only batch.
                          const needsQueue = stagedBatches > 1 && onQueueProduction;
                          if (onSubmitBatch && !needsQueue) {
                            setBatchRunning(true);
                            setBatchError(null);
                            try {
                              await onSubmitBatch({
                                kind: "production",
                                destination: systemId,
                                steps: productionOptions.flatMap((opt) =>
                                  Array.from({ length: draft[opt.id] ?? 0 }, () => ({
                                    kind: "produce" as const,
                                    unit: String(opt.payload?.unit ?? opt.id),
                                    count: Number(opt.payload?.count ?? 1),
                                  })),
                                ),
                              });
                              setDraft({});
                            } catch (error) {
                              setBatchError(error instanceof Error ? error.message : String(error));
                            } finally {
                              setBatchRunning(false);
                            }
                            return;
                          }
                          const units = productionOptions.flatMap((opt) =>
                            Array.from({ length: draft[opt.id] ?? 0 }, () =>
                              typeof opt.payload?.unit === "string" ? opt.payload.unit : opt.id,
                            ),
                          );
                          if (onQueueProduction) onQueueProduction(units);
                          else
                            void submitDirect(
                              productionOptions.find((opt) => (draft[opt.id] ?? 0) > 0)!.id,
                            );
                          setDraft({});
                        }}
                      >
                        {queued ? `Building (${queuedUnits.length} remaining)…` : "Confirm builds"}
                      </button>
                      {declineOption && (
                        <button
                          type="button"
                          data-testid="done-producing-btn"
                          onClick={() => submitDirect(declineOption.id)}
                          disabled={isSubmitting || queued || stagedBatches > 0}
                          className="button button--primary production-drawer__done"
                        >
                          {declineOption.label || "Done Producing"}
                        </button>
                      )}
                    </div>
                  </div>
                )}

                {/* Place Unit Mode */}
                {isActor && isPlaceUnit && (
                  <div className="workflow-stack">
                    <div className="workflow-copy">
                      {choice.prompt}. Select destination location:
                    </div>

                    <div className="workflow-stack workflow-stack--compact">
                      {productionOptions.map((opt) => (
                        <button
                          key={opt.id}
                          type="button"
                          data-testid={`place-spot-btn-${opt.id}`}
                          onClick={() => submitDirect(opt.id)}
                          disabled={isSubmitting}
                          className="button button--secondary workflow-button--wide"
                        >
                          {opt.label}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </WorkflowShell>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
