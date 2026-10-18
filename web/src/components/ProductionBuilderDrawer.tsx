import { ARMADA_TEXT, readFleetSupply } from "../presentation/commandTokens.ts";
import React, { useEffect, useMemo, useState } from "react";
import { ValueUnit } from "./PlanetValueIcons.tsx";
import { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { Dialog } from "../primitives/index.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { MechInfoRow, UnitInfoButton } from "./UnitInfo.tsx";
import { UnitIcon, getUnitBaseType } from "./UnitIcon.tsx";
import { UnitBuildStats, useBuildUnit } from "./UnitBuildStats.tsx";
import {
  draftResourceCost,
  productionTotal,
  isExchangeOption,
  optionUnit,
  optionCapacity,
  planBuilds,
  produceStep,
} from "../presentation/productionDraft.ts";
import {
  capacityMeter,
  capacityMeterText,
  readCapacityFacts,
  readCapacityNow,
} from "../presentation/productionCapacity.ts";
import { usePreparedHint } from "../presentation/PreparedHint.tsx";
import { useSinglePaymentSetting } from "../hooks/useSinglePaymentSetting.ts";
import { ProductionTotal } from "./ProductionTotal.tsx";
import { isDryNonce } from "../presentation/dryChoice.ts";
import { TradeGoodSources } from "./TradeGoodSources.tsx";

export interface ProductionBuilderDrawerProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  /** The producing seat's trade goods: the part of the resource budget that is not from planets. */
  tradeGoods?: number;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
  queuedUnits?: readonly string[];
  onQueueProduction?: (units: string[]) => void;
  onSubmitBatch?: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
}

type OptionGroupKey = "ships" | "ground" | "structures";
const GROUP_TITLES: Record<OptionGroupKey, string> = {
  ships: "Ships",
  ground: "Ground forces",
  structures: "Structures",
};
const GROUP_OF: Record<string, OptionGroupKey> = {
  infantry: "ground",
  mech: "ground",
  pds: "structures",
  spacedock: "structures",
};

const unitKeyOf = (option: ChoiceOption): string => String(option.payload?.unit ?? option.id);

/** Build options split into ships, ground forces and structures; each keeps the order the engine offered. */
function groupOptions(options: ChoiceOption[]) {
  const groups = new Map<OptionGroupKey, ChoiceOption[]>();
  for (const option of options) {
    const key = GROUP_OF[getUnitBaseType(unitKeyOf(option))] ?? "ships";
    groups.set(key, [...(groups.get(key) ?? []), option]);
  }
  return (["ships", "ground", "structures"] as const)
    .filter((key) => groups.has(key))
    .map((key) => ({ key, title: GROUP_TITLES[key], options: groups.get(key)! }));
}

const BuildOptionCard: React.FC<{
  option: ChoiceOption;
  seat: string;
  count: number;
  blockedReason: React.ReactNode;
  disabledAdd: boolean;
  disabledRemove: boolean;
  /** Units of this build that would not fit the system's capacity and would be removed. */
  capacityOver?: boolean;
  onAdd: () => void;
  onRemove: () => void;
}> = ({ option, seat, count, blockedReason, disabledAdd, disabledRemove, capacityOver, onAdd, onRemove }) => {
  const unit = unitKeyOf(option);
  const built = useBuildUnit(unit, seat);
  // The Amalgamation exchange is the same unit bought another way: no resources, one captured
  // model of the type returned. It sits beside the paid build of that type, never replaces it.
  const exchange = isExchangeOption(option);
  const label = exchange ? `${built?.name ?? unit} (exchange)` : option.label.replace(/^produce\s+/i, "");
  const name = built?.name ?? label;
  const payload = option.payload ?? {};
  const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : undefined);
  return (
    <div
      className="workflow-card production-drawer__unit"
      data-testid={`produce-option-${option.id}`}
      data-blocked={blockedReason ? "true" : undefined}
      data-exchange={exchange ? "true" : undefined}
      data-capacity-over={capacityOver ? "true" : undefined}
    >
      <div className="production-drawer__unit-head">
        <span className="production-drawer__unit-name">
          <UnitIcon type={unit} size={22} className="production-drawer__unit-icon" aria-hidden="true" />
          {name}
          {exchange && (
            <span className="production-drawer__exchange-tag" data-testid={`produce-exchange-tag-${option.id}`}>
              Exchange
            </span>
          )}
          <UnitInfoButton unit={unit} seat={seat} name={name} />
        </span>
        <div className="workflow-row">
          <button
            type="button"
            className="button button--secondary button--icon"
            aria-label={`Remove ${label}`}
            disabled={disabledRemove}
            onClick={onRemove}
          >
            −
          </button>
          <span className="workflow-count" data-testid={`produce-count-${option.id}`}>
            {count}
          </span>
          <button
            type="button"
            className="button button--secondary button--icon"
            data-testid={`produce-unit-btn-${option.id}`}
            aria-label={`Add ${label}`}
            disabled={disabledAdd}
            onClick={onAdd}
          >
            +
          </button>
        </div>
      </div>
      {exchange && (
        <p className="production-drawer__exchange-note" data-testid={`produce-exchange-note-${option.id}`}>
          Return a captured {built?.name ?? unit} to produce it for no resources (Amalgamation). It still
          uses production capacity.
        </p>
      )}
      <UnitBuildStats
        unit={unit}
        seat={seat}
        name={name}
        price={{
          cost: num(payload.cost),
          printedCost: num(payload.printed_cost),
          count: num(payload.count),
          free: payload.free_this_use === true || exchange,
        }}
      />
      {capacityOver && (
        <p className="production-drawer__capacity-warning" data-testid={`produce-capacity-warning-${option.id}`}>
          Over capacity: these would be removed.
        </p>
      )}
      {blockedReason && (
        <p className="production-drawer__blocked" data-testid="produce-blocked">
          {blockedReason}
        </p>
      )}
    </div>
  );
};

export const ProductionBuilderDrawer: React.FC<ProductionBuilderDrawerProps> = ({
  choice,
  model,
  viewerSeat,
  tradeGoods,
  onSubmit,
  isOpen,
  onClose,
  lastError,
  queuedUnits = [],
  onQueueProduction,
  onSubmitBatch,
}) => {
  const display = usePlayerIdentity();
  const { enabled: singlePayment, setEnabled: setSinglePayment } = useSinglePaymentSetting();
  const [draft, setDraft] = useState<Record<string, number>>({});
  const [batchRunning, setBatchRunning] = useState(false);
  const [batchError, setBatchError] = useState<string | null>(null);
  // A prepared Warfare secondary opens the real builder with its planned builds already staged;
  // the confirm stays the player's (or Auto's, which sends the same batch).
  const prepared = usePreparedHint();
  useEffect(() => {
    const planned =
      choice && prepared?.builds?.length ? planBuilds(choice, prepared.builds) : null;
    setDraft(planned?.ok ? planned.draft : {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [choice?.nonce, prepared?.builds?.join(",")]);
  // While preparing, the stand-in is answered into the plan: there is no build queue to run.
  const preparing = isDryNonce(choice?.nonce);
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
      (draft[opt.id] ?? 0) * optionCapacity(opt),
    0,
  );
  const stagedCost = draftResourceCost(productionOptions, draft);
  const total = productionTotal(productionOptions, draft);
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

  // Extract fleet supply if available from context
  // The server sends it as a display-only choice detail.
  const fleetSupply = choice?.details?.fleet_supply ?? null;
  // Ships the staged builds add to the pool. The engine names them per option (`fleet_ships`:
  // non-fighter ships only), so this never re-derives which units count.
  const stagedShips = productionOptions.reduce((sum, opt) => {
    const ships = opt.payload?.fleet_ships;
    return sum + (draft[opt.id] ?? 0) * (typeof ships === "number" && ships > 0 ? ships : 0);
  }, 0);
  const fleetNow = readFleetSupply(fleetSupply);
  const fleetUnlimited =
    !!fleetSupply && typeof fleetSupply === "object" && (fleetSupply as { unlimited?: unknown }).unlimited === true;
  const overFleetSupply =
    !!fleetNow && !fleetUnlimited && stagedShips > 0 && fleetNow.used + stagedShips > fleetNow.limit;
  const overFleetText = fleetNow
    ? `These builds would put ${fleetNow.used + stagedShips} ships in this system against a fleet supply of ${fleetNow.limit}. You may still build them, but at the end of your turn you must remove the ships above the limit.`
    : "";

  // Capacity (LRR 16) of the producing system with the staged builds. The engine names the
  // present position and what each build gives and takes; this only adds them up.
  const capacityNow = readCapacityNow(choice?.details?.capacity);
  const capacity = capacityNow ? capacityMeter(capacityNow, productionOptions, draft) : null;
  const overCapacity = !!capacity && capacity.over > 0 && capacity.queuedFighters + capacity.queuedGround > 0;
  const overCapacityText = capacity
    ? `${capacityMeterText(capacity)}. You may still build them, but the units above the capacity are removed when production ends.`
    : "";

  const optionGroups = groupOptions(productionOptions);
  const multiGroup = optionGroups.length > 1;

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
            choice={choice}
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
                    {/* Fleet Supply Display */}
                    <div className="workflow-card production-drawer__meter">
                      <div className="workflow-card--row">
                        <span className="text-muted">Fleet Supply:</span>
                        <span className="token-panel__armada-group">
                        <span
                          data-testid="fleet-supply-counter"
                          className={overFleetSupply ? "production-drawer__fleet--over" : "text-success"}
                          data-over={overFleetSupply ? "true" : undefined}
                        >
                          {fleetNow ? (
                            <>
                              {fleetNow.used}
                              {stagedShips > 0 && (
                                <>
                                  {" "}
                                  <span className="production-drawer__fleet-staged" data-testid="fleet-supply-staged">
                                    +{stagedShips}
                                  </span>
                                </>
                              )}
                              {" / "}
                              {fleetUnlimited ? "unlimited" : fleetNow.limit}
                            </>
                          ) : (
                            "Data unavailable"
                          )}
                        </span>
                        {(readFleetSupply(fleetSupply)?.fleetSupplyBonus ?? 0) > 0 && (
                          <span
                            className="token-panel__armada"
                            data-testid="production-armada"
                            title={ARMADA_TEXT}
                            aria-label={ARMADA_TEXT}
                          >
                            incl. +{readFleetSupply(fleetSupply)?.fleetSupplyBonus} Armada
                          </span>
                        )}
                        </span>
                      </div>
                    </div>

                    {/* Ship capacity: transport of the ships there + queued, against what rides in it */}
                    {capacity && (
                      <div className="workflow-card production-drawer__meter" data-testid="capacity-meter">
                        <div className="workflow-card--row">
                          <span
                            data-testid="capacity-counter"
                            className={overCapacity ? "production-drawer__fleet--over" : "text-success"}
                            data-over={overCapacity ? "true" : undefined}
                          >
                            {capacityMeterText(capacity)}
                          </span>
                        </div>
                        <p className="production-drawer__split" data-testid="capacity-breakdown">
                          <span>
                            Ships there: <strong>{capacity.existing}</strong>
                          </span>
                          {capacity.fromQueued > 0 && (
                            <span>
                              Queued ships: <strong className="production-drawer__fleet-staged">+{capacity.fromQueued}</strong>
                            </span>
                          )}
                          <span>
                            Needing it: <strong>{capacity.consumed}</strong>
                            {capacity.queuedFighters + capacity.queuedGround > 0 && (
                              <>
                                {" "}
                                (<span className="production-drawer__fleet-staged">
                                  +{capacity.queuedFighters + capacity.queuedGround} queued
                                </span>)
                              </>
                            )}
                          </span>
                        </p>
                        {capacity.maybeGround > 0 && (
                          <p className="text-muted" data-testid="capacity-maybe">
                            +{capacity.maybeGround} more if you place the ground forces in space.
                          </p>
                        )}
                      </div>
                    )}

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
                        <span className="text-muted">
                          <ValueUnit kind="resources" size="bar" /> budget
                        </span>
                        <span data-testid="production-resources-counter" className="text-success">
                          {resourceLimit === null
                            ? "Unavailable"
                            : (
                              <>
                                {stagedCost} / {resourceLimit} <ValueUnit kind="resources" /> (
                                {Math.max(0, resourceLimit - stagedCost)} Left)
                              </>
                            )}
                        </span>
                      </div>
                      {resourceLimit !== null && (
                        <progress
                          className="production-drawer__progress"
                          max={Math.max(1, resourceLimit)}
                          value={stagedCost}
                        />
                      )}
                      {resourceLimit !== null && typeof tradeGoods === "number" && (
                        // The budget is planets plus trade goods; the engine sends only the sum.
                        <>
                          <p className="production-drawer__split" data-testid="production-resources-split">
                            <span>
                              Planets:{" "}
                              <strong data-testid="production-split-planets">
                                {Math.max(0, resourceLimit - tradeGoods)}
                              </strong>{" "}
                              <ValueUnit kind="resources" />
                            </span>
                            <span>
                              Trade goods: <strong data-testid="production-split-tg">{Math.min(tradeGoods, resourceLimit)}</strong>
                            </span>
                          </p>
                          <TradeGoodSources seat={choice.actor} />
                        </>
                      )}
                    </div>

                    <MechInfoRow seat={choice.actor} />

                    {/* Build options, grouped by where the unit fights */}
                    <div data-testid="production-options-grid" className="production-drawer__groups">
                      {optionGroups.map((group) => (
                        <section
                          key={group.key}
                          aria-label={multiGroup ? group.title : undefined}
                          data-testid={`production-group-${group.key}`}
                        >
                          {multiGroup && (
                            <h3 className="production-drawer__group-title">{group.title}</h3>
                          )}
                          <div className="production-drawer__group">
                            {group.options.map((opt) => {
                              const capacity = optionCapacity(opt);
                              const count = draft[opt.id] ?? 0;
                              const nextCost = draftResourceCost(productionOptions, {
                                ...draft,
                                [opt.id]: count + 1,
                              });
                              const capacityBlocked =
                                capacity < 0 || stagedCapacity + capacity > capacityRemaining;
                              const resourceBlocked =
                                resourceLimit !== null && nextCost > resourceLimit;
                              const blocked = capacityBlocked
                                ? "No production capacity left"
                                : resourceBlocked
                                  ? (
                                      <>
                                        Needs {nextCost - (resourceLimit ?? 0)} more <ValueUnit kind="resources" />
                                      </>
                                    )
                                  : null;
                              return (
                                <BuildOptionCard
                                  key={opt.id}
                                  option={opt}
                                  seat={choice.actor}
                                  count={count}
                                  blockedReason={blocked}
                                  disabledAdd={
                                    queued ||
                                    isSubmitting ||
                                    capacityBlocked ||
                                    resourceLimit === null ||
                                    resourceBlocked
                                  }
                                  disabledRemove={!count || queued || isSubmitting}
                                  capacityOver={
                                    overCapacity &&
                                    count > 0 &&
                                    readCapacityFacts(opt)?.placement === "always" &&
                                    readCapacityFacts(opt)?.role !== "none"
                                  }
                                  onAdd={() =>
                                    setDraft((prev) => ({ ...prev, [opt.id]: count + 1 }))
                                  }
                                  onRemove={() =>
                                    setDraft((prev) => ({ ...prev, [opt.id]: count - 1 }))
                                  }
                                />
                              );
                            })}
                          </div>
                        </section>
                      ))}
                    </div>

                    <div className="production-drawer__footer">
                      <ProductionTotal
                        total={total}
                        available={resourceLimit}
                        singlePayment={singlePayment && !preparing}
                        partial={capacitySpent > 0}
                      />
                      {stagedBatches > 1 && !preparing && (
                        <p className="text-muted" data-testid="production-queue-note">
                          {singlePayment
                            ? `You pay once for all ${stagedBatches} builds (${stagedCost} resources); the later payments follow your choice. Placement is still asked per unit, and anything unexpected stops the queue.`
                            : "Staged builds submit one decision at a time. The queue pauses for payment or placement and stops if a later offer changes."}
                        </p>
                      )}
                      {!preparing && (
                        <label className="production-drawer__setting" data-testid="ask-each-payment-setting">
                          <input
                            type="checkbox"
                            data-testid="ask-each-payment-toggle"
                            checked={!singlePayment}
                            onChange={(event) => setSinglePayment(!event.target.checked)}
                          />{" "}
                          Ask for each payment
                        </label>
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
                        className={
                          overFleetSupply || overCapacity
                            ? "button button--primary production-drawer__build--over"
                            : "button button--primary"
                        }
                        data-over-fleet-supply={overFleetSupply ? "true" : undefined}
                        data-over-capacity={overCapacity ? "true" : undefined}
                        title={
                          [overFleetSupply ? overFleetText : "", overCapacity ? overCapacityText : ""]
                            .filter(Boolean)
                            .join(" ") || undefined
                        }
                        aria-label={queued || preparing ? undefined : "Confirm builds"}
                        disabled={!stagedBatches || queued || isSubmitting || batchRunning}
                        onClick={async () => {
                          // A build can open a payment or placement decision before
                          // another build is offered. The shell's queue waits for that choice
                          // to resolve instead of sending an invalid produce-only batch.
                          const needsQueue = stagedBatches > 1 && onQueueProduction && !preparing;
                          if (onSubmitBatch && !needsQueue) {
                            setBatchRunning(true);
                            setBatchError(null);
                            try {
                              await onSubmitBatch({
                                kind: "production",
                                destination: systemId,
                                steps: productionOptions.flatMap((opt) =>
                                  Array.from({ length: draft[opt.id] ?? 0 }, () => produceStep(opt)),
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
                              optionUnit(opt),
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
                        {queued
                          ? `Building (${queuedUnits.length} remaining)…`
                          : preparing
                            ? "Save these builds"
                            : total.total !== null && stagedBatches > 0
                              ? `Build (${total.total})`
                              : "Confirm builds"}
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
