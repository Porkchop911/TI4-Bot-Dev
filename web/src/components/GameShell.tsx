import React, { useEffect, useState, useMemo, useRef } from "react";
import { GameLogEntry } from "../hooks/useGameSession.ts";
import {
  BoardView,
  CurrentLogPath,
  HistoryStatus,
  PendingChoiceDto,
  PlayerView,
  ObjectiveProgressView,
} from "../protocol/types.ts";
import { EventLog } from "./EventLog.tsx";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { TechnologyModal } from "./TechnologyModal.tsx";
import { ObjectivesModal } from "./ObjectivesModal.tsx";
import { PaymentDrawer } from "./PaymentDrawer.tsx";
import {
  TacticalMovementOverlay,
  emptyMovementPlan,
  type ExecutionPlan,
} from "./TacticalMovementOverlay.tsx";
import { SpaceCombatOverlay } from "./CombatResolutionModal.tsx";
import { TradeDeskModal } from "./TradeDeskModal.tsx";
import { AgendaBallotModal } from "./AgendaBallotModal.tsx";
import { ReactionStatusBar } from "./ReactionStatusBar.tsx";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";
import { CargoLoadingTray } from "./CargoLoadingTray.tsx";
import { InvasionLandingTray } from "./InvasionLandingTray.tsx";
import type { Landing } from "./InvasionLandingTray.tsx";
import { InvasionOverlay } from "./InvasionOverlay.tsx";
import { SystemActivationBar } from "./SystemActivationBar.tsx";
import { deriveChoiceRendererModel, ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { Dialog, overlayStack } from "../primitives/index.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { PipelineRunnerContext, useOwnedPipelineRunner } from "../hooks/usePipelineRunner.ts";

export interface GameShellProps {
  header: React.ReactNode;
  board: React.ReactNode;
  playerSheet: React.ReactNode;
  detail?: React.ReactNode;
  events: GameLogEntry[];
  history?: HistoryStatus;
  currentPath?: CurrentLogPath;
  logHistoryKey?: unknown;
  onChangeHistory?: (action: import("../protocol/client.ts").HistoryChange, steps?: number) => void;
  historyBusy?: boolean;
  choice: PendingChoiceDto | null;
  onSubmitChoice: (optionId: string) => Promise<void>;
  onSubmitMovementBatch?: (
    destination: string,
    steps: import("../protocol/client.ts").MovementStep[],
  ) => Promise<void>;
  onSubmitBasketBatch?: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
  lastError?: string | null;
  selectedOptionId?: string;
  selectedSystemId?: string | null;
  onSelectOption?: (optionId: string) => void;
  viewerSeat?: string | null;
  players?: Record<string, PlayerView> | PlayerView[];
  boardView?: BoardView;
  activeSystemId?: string | null;
  revealedObjectives?: readonly string[];
  scoredObjectives?: Record<string, string[]>;
  objectiveProgress?: Record<string, Record<string, ObjectiveProgressView>>;
  productionQueue?: readonly string[];
  productionError?: string | null;
  onQueueProduction?: (units: string[]) => void;
}

export interface ChoiceRendererDispatcherProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  onSubmitMovementBatch?: GameShellProps["onSubmitMovementBatch"];
  onSubmitBasketBatch?: GameShellProps["onSubmitBasketBatch"];
  lastError?: string | null;
  selectedOptionId?: string;
  selectedSystemId?: string | null;
  onSelectOption?: (optionId: string) => void;
  isMinimized: boolean;
  onMinimizedChange: (minimized: boolean) => void;
  players?: Record<string, PlayerView>;
  boardView?: BoardView;
  activeSystemId?: string | null;
  revealedObjectives?: readonly string[];
  scoredObjectives?: Record<string, string[]>;
  objectiveProgress?: Record<string, Record<string, ObjectiveProgressView>>;
  productionQueue?: readonly string[];
  productionError?: string | null;
  onQueueProduction?: (units: string[]) => void;
  tacticalPlan?: React.RefObject<ExecutionPlan>;
  tacticalStep?: number;
  onTacticalStep?: () => void;
  landingDraft?: Landing[];
  onLandingDraftChange?: (draft: Landing[]) => void;
}

type WorkflowRenderer = (
  props: Omit<ChoiceRendererDispatcherProps, "model"> & {
    choice: PendingChoiceDto;
    model: ChoiceRendererModel | null;
  },
) => React.ReactNode;

const renderTactical: WorkflowRenderer = ({
  choice,
  model,
  boardView,
  activeSystemId,
  viewerSeat,
  players,
  onSubmit,
  onSubmitMovementBatch,
  isMinimized,
  onMinimizedChange,
  lastError,
  tacticalPlan,
  tacticalStep,
  onTacticalStep,
}) => (
  <TacticalMovementOverlay
    key="tactical_movement_overlay"
    choice={choice}
    model={model}
    board={boardView}
    viewerSeat={viewerSeat}
    activeSystemId={
      activeSystemId ||
      (model?.selectionMode.mode === "tactical_move" ||
      model?.selectionMode.mode === "tactical_cargo"
        ? model.selectionMode.activeSystem
        : choice.context?.target && "System" in choice.context.target
          ? choice.context.target.System
          : null)
    }
    player={players?.[choice.actor] ?? null}
    onSubmit={onSubmit}
    onSubmitBatch={onSubmitMovementBatch}
    isOpen={!isMinimized}
    onClose={() => onMinimizedChange(true)}
    lastError={lastError}
    executionPlan={tacticalPlan}
    executionStep={tacticalStep}
    onExecutionStep={onTacticalStep}
  />
);

const workflowRenderers = new Map<ChoiceRendererModel["workflow"], WorkflowRenderer>([
  [
    "payment",
    ({
      choice,
      model,
      viewerSeat,
      players,
      onSubmit,
      onSubmitBasketBatch,
      isMinimized,
      onMinimizedChange,
      lastError,
      selectedOptionId,
    }) => (
      <PaymentDrawer
        choice={choice}
        model={model}
        viewerSeat={viewerSeat}
        player={players?.[choice.actor] ?? null}
        onSubmit={onSubmit}
        onSubmitBatch={onSubmitBasketBatch}
        isOpen={!isMinimized}
        onClose={() => onMinimizedChange(true)}
        lastError={lastError}
        selectedOptionId={selectedOptionId}
      />
    ),
  ],
  ["tactical_movement", renderTactical],
  [
    "tactical_cargo",
    (props) =>
      props.tacticalPlan?.current.active ? (
        renderTactical(props)
      ) : (
        <CargoLoadingTray
          choice={props.choice}
          board={props.boardView}
          onSubmit={props.onSubmit}
          isOpen={!props.isMinimized}
          onClose={() => props.onMinimizedChange(true)}
          lastError={props.lastError}
        />
      ),
  ],
  ["combat_sustain", renderCombat],
  ["combat_casualty", renderCombat],
  ["combat_retreat", renderCombat],
  ["transaction_propose", renderTrade],
  ["transaction_answer", renderTrade],
  ["agenda_vote_outcome", renderAgenda],
  ["agenda_vote_planets", renderAgenda],
  [
    "action_card_reaction",
    ({ choice, model, viewerSeat, onSubmit, isMinimized, onMinimizedChange, lastError }) => (
      <ReactionStatusBar
        choice={choice}
        model={model}
        viewerSeat={viewerSeat}
        onSubmit={onSubmit}
        isOpen={!isMinimized}
        onClose={() => onMinimizedChange(true)}
        lastError={lastError}
      />
    ),
  ],
  [
    "production",
    ({
      choice,
      model,
      viewerSeat,
      onSubmit,
      onSubmitBasketBatch,
      isMinimized,
      onMinimizedChange,
      lastError,
      productionQueue,
      productionError,
      onQueueProduction,
    }) => (
      <ProductionBuilderDrawer
        choice={choice}
        model={model}
        viewerSeat={viewerSeat}
        onSubmit={onSubmit}
        onSubmitBatch={onSubmitBasketBatch}
        isOpen={!isMinimized}
        onClose={() => onMinimizedChange(true)}
        lastError={productionError || lastError}
        queuedUnits={productionQueue}
        onQueueProduction={onQueueProduction}
      />
    ),
  ],
  ["generic_selection", renderGeneric],
  ["objective_scoring", renderObjectiveScoring],
  ["strategy_card_draft", renderGeneric],
  [
    "system_activation",
    ({
      choice,
      model,
      viewerSeat,
      selectedOptionId,
      selectedSystemId,
      onSelectOption,
      onSubmit,
      boardView,
      lastError,
    }) => (
      <SystemActivationBar
        choice={choice}
        model={model}
        viewerSeat={viewerSeat}
        selectedOptionId={selectedOptionId}
        selectedSystemId={selectedSystemId}
        onSelectOption={onSelectOption}
        onSubmit={onSubmit}
        boardView={boardView}
        lastError={lastError}
      />
    ),
  ],
  [
    "tactical_invasion",
    ({
      choice,
      boardView,
      viewerSeat,
      selectedOptionId,
      onSubmit,
      isMinimized,
      onMinimizedChange,
      lastError,
    }) =>
      !isMinimized && (
        <InvasionLandingTray
          choice={choice}
          board={boardView}
          viewerSeat={viewerSeat}
          selectedOptionId={selectedOptionId}
          onSubmit={onSubmit}
          onClose={() => onMinimizedChange(true)}
          lastError={lastError}
        />
      ),
  ],
  ["technology_research", renderTechnologyResearch],
]);

function renderObjectiveScoring({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isMinimized,
  onMinimizedChange,
  lastError,
  players,
  revealedObjectives,
  scoredObjectives,
  objectiveProgress,
  selectedOptionId,
  onSelectOption,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <ObjectivesModal
      isOpen={!isMinimized}
      onClose={() => onMinimizedChange(true)}
      revealedObjectives={revealedObjectives}
      scoredObjectives={scoredObjectives}
      objectiveProgress={objectiveProgress}
      players={players}
      viewerSeat={viewerSeat}
      choice={choice}
      model={model}
      onSubmit={onSubmit}
      lastError={lastError}
      selectedOptionId={selectedOptionId}
      onSelectOption={onSelectOption}
      isScoringMode={true}
    />
  );
}

function renderTechnologyResearch({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isMinimized,
  onMinimizedChange,
  lastError,
  boardView,
  players,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <TechnologyModal
      isOpen={!isMinimized}
      onClose={() => onMinimizedChange(true)}
      choice={choice}
      model={model}
      viewerSeat={viewerSeat}
      onSubmit={onSubmit}
      board={boardView}
      players={players}
      isResearchMode={true}
      lastError={lastError}
    />
  );
}

function renderCombat({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isMinimized,
  onMinimizedChange,
  lastError,
  boardView,
  players,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <SpaceCombatOverlay
      choice={choice}
      model={model}
      viewerSeat={viewerSeat}
      onSubmit={onSubmit}
      isOpen={!isMinimized}
      onClose={() => onMinimizedChange(true)}
      isMinimized={isMinimized}
      onMinimize={onMinimizedChange}
      lastError={lastError}
      board={boardView}
      players={players}
    />
  );
}

function renderTrade({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isMinimized,
  onMinimizedChange,
  lastError,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <TradeDeskModal
      choice={choice}
      model={model}
      viewerSeat={viewerSeat}
      onSubmit={onSubmit}
      isOpen={!isMinimized}
      onClose={() => onMinimizedChange(true)}
      lastError={lastError}
    />
  );
}

function renderAgenda({
  choice,
  model,
  viewerSeat,
  onSubmit,
  onSubmitBasketBatch,
  isMinimized,
  onMinimizedChange,
  lastError,
  selectedOptionId,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <AgendaBallotModal
      choice={choice}
      model={model}
      viewerSeat={viewerSeat}
      onSubmit={onSubmit}
      onSubmitBatch={onSubmitBasketBatch}
      isOpen={!isMinimized}
      onClose={() => onMinimizedChange(true)}
      lastError={lastError}
      selectedOptionId={selectedOptionId}
    />
  );
}

function renderGeneric({
  choice,
  model,
  onSubmit,
  lastError,
  isMinimized,
  onMinimizedChange,
  selectedOptionId,
  onSelectOption,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <PendingChoiceModal
      choice={choice}
      model={model}
      onSubmit={onSubmit}
      lastError={lastError}
      isMinimized={isMinimized}
      onMinimizedChange={onMinimizedChange}
      selectedOptionId={selectedOptionId}
      onSelectOption={onSelectOption}
    />
  );
}

export const ChoiceRendererDispatcher: React.FC<ChoiceRendererDispatcherProps> = ({
  choice,
  model: propModel,
  viewerSeat,
  onSubmit,
  onSubmitMovementBatch,
  onSubmitBasketBatch,
  lastError,
  selectedOptionId,
  selectedSystemId,
  onSelectOption,
  isMinimized,
  onMinimizedChange,
  players,
  boardView,
  activeSystemId,
  revealedObjectives,
  scoredObjectives,
  objectiveProgress,
  productionQueue,
  productionError,
  onQueueProduction,
  tacticalPlan,
  tacticalStep,
  onTacticalStep,
  landingDraft,
  onLandingDraftChange,
}) => {
  const present = useParticipantText();
  const derivedModel = useMemo(() => {
    return choice ? deriveChoiceRendererModel(choice, viewerSeat ?? null) : null;
  }, [choice, viewerSeat]);

  const model = propModel ?? derivedModel;
  const workflow = model?.workflow ?? "generic_selection";
  const combatSubtype = choice?.context?.subtype;
  const spectatorCombatWorkflow =
    combatSubtype === "sustain_damage" ||
    combatSubtype === "assign_casualty" ||
    combatSubtype === "announce_retreat" ||
    combatSubtype === "retreat_to";
  const isBattleChoice = Boolean(boardView?.combat && choice?.context?.space_battle);

  const isCombatWorkflow =
    spectatorCombatWorkflow ||
    isBattleChoice ||
    workflow === "combat_sustain" ||
    workflow === "combat_casualty" ||
    workflow === "combat_retreat";

  if (boardView?.invasion && !boardView.combat) {
    const isActor = choice?.actor === viewerSeat;
    return isMinimized ? (
      <div className="choice-banner">
        <button
          type="button"
          data-testid="resume-decision-btn"
          className="button button--primary choice-minimized-pill"
          onClick={() => onMinimizedChange(false)}
        >
          ⚔️{" "}
          {isActor && choice
            ? `Resume: ${choice.prompt}`
            : `View invasion · System ${boardView.invasion.system_id}`}
        </button>
      </div>
    ) : (
      <InvasionOverlay
        board={boardView}
        choice={choice}
        players={players}
        viewerSeat={viewerSeat}
        onSubmit={onSubmit}
        onClose={() => onMinimizedChange(true)}
        lastError={lastError}
        landingDraft={landingDraft}
        onLandingDraftChange={onLandingDraftChange}
      />
    );
  }

  // If there is an active combat on the board without a pending choice, render combat overlay in spectator mode
  if (!choice && boardView?.combat) {
    return (
      <SpaceCombatOverlay
        choice={null}
        model={null}
        viewerSeat={viewerSeat}
        onSubmit={onSubmit}
        isOpen={!isMinimized}
        onClose={() => onMinimizedChange(true)}
        isMinimized={isMinimized}
        onMinimize={onMinimizedChange}
        lastError={lastError}
        board={boardView}
        players={players}
      />
    );
  }

  if (!choice || (viewerSeat !== undefined && choice.actor !== viewerSeat && !isCombatWorkflow))
    return null;

  // A view-only copy: IDs, payloads and the original pending choice stay intact.
  const visibleChoice = {
    ...choice,
    prompt: present(choice.prompt),
    options: choice.options.map((option) => ({
      ...option,
      label: present(option.label),
      description: option.description == null ? option.description : present(option.description),
    })),
  };

  const renderer =
    isBattleChoice || (spectatorCombatWorkflow && !model)
      ? renderCombat
      : (workflowRenderers.get(workflow) ?? workflowRenderers.get("generic_selection")!);
  const wrappedWorkflow =
    workflow === "payment" ||
    workflow === "tactical_movement" ||
    workflow === "tactical_cargo" ||
    workflow === "action_card_reaction";
  const content = renderer({
    choice: visibleChoice,
    model,
    viewerSeat,
    onSubmit,
    onSubmitMovementBatch,
    onSubmitBasketBatch,
    lastError: lastError ? present(lastError) : lastError,
    selectedOptionId,
    selectedSystemId,
    onSelectOption,
    isMinimized,
    onMinimizedChange,
    players,
    boardView,
    activeSystemId,
    revealedObjectives,
    scoredObjectives,
    objectiveProgress,
    productionQueue,
    productionError,
    onQueueProduction,
    tacticalPlan,
    tacticalStep,
    onTacticalStep,
  });

  return (
    <>
      {/* Minimized Decision Pill for dedicated drawers/modals */}
      {isMinimized &&
        ![
          "generic_selection",
          "strategy_card_draft",
          "system_activation",
          "combat_sustain",
          "combat_casualty",
          "combat_retreat",
        ].includes(workflow) && (
          <div className="choice-banner choice-minimized-pill" data-testid="choice-minimized-pill">
            <span className="choice-minimized-pill__prompt">
              {workflow === "technology_research"
                ? "Research Technology"
                : workflow === "objective_scoring"
                  ? "Score Objective"
                  : workflow === "tactical_movement" || visibleChoice.prompt === "movement"
                    ? "Move Units"
                    : workflow === "tactical_cargo" || visibleChoice.prompt === "load_cargo"
                      ? "Load Cargo"
                      : visibleChoice.prompt}
            </span>
            <button
              type="button"
              data-testid="resume-decision-btn"
              onClick={() => onMinimizedChange(false)}
              className="button button--primary button--sm"
            >
              Resume decision
            </button>
          </div>
        )}

      {wrappedWorkflow ? (
        <Dialog.Root open={!isMinimized} onOpenChange={(open) => onMinimizedChange(!open)}>
          <Dialog.Content
            keepMounted
            className="decision-modal choice-workflow-dialog"
            data-testid="decision-modal"
          >
            <div className="decision-modal__panel panel">
              <Dialog.Title as="h2" className="visually-hidden">
                {visibleChoice.prompt}
              </Dialog.Title>
              {content}
            </div>
          </Dialog.Content>
        </Dialog.Root>
      ) : (
        content
      )}
    </>
  );
};

export const GameShell: React.FC<GameShellProps> = ({
  header,
  board,
  playerSheet,
  detail,
  events,
  history,
  currentPath,
  logHistoryKey,
  onChangeHistory,
  historyBusy,
  choice,
  onSubmitChoice,
  onSubmitMovementBatch,
  onSubmitBasketBatch,
  lastError,
  selectedOptionId,
  selectedSystemId,
  onSelectOption,
  viewerSeat,
  players,
  boardView,
  activeSystemId,
  revealedObjectives,
  scoredObjectives,
  objectiveProgress,
}) => {
  const [openDrawer, setOpenDrawer] = useState<"events" | "players" | null>(null);
  const [isChoiceMinimized, setIsChoiceMinimized] = useState(false);
  const [productionQueue, setProductionQueue] = useState<{
    actor: string;
    system: string;
    units: string[];
  } | null>(null);
  const [productionError, setProductionError] = useState<string | null>(null);
  const [landingDraftState, setLandingDraftState] = useState<{
    key: string;
    entries: Landing[];
  } | null>(null);
  const landingKey = boardView?.invasion
    ? `${history?.generation ?? 0}:${boardView.invasion.system_id}:${boardView.invasion.invasion_seq}`
    : null;
  const submittedNonce = useRef<string | null>(null);
  const productionSubmitting = useRef(false);
  const tacticalPlan = useRef<ExecutionPlan>(emptyMovementPlan());
  const [tacticalStep, setTacticalStep] = useState(0);
  const lastHistoryGeneration = useRef(history?.generation);
  const pipelineRunner = useOwnedPipelineRunner(choice, onSubmitChoice);

  // A restored timeline must not resume a movement plan from the old timeline.
  if (history?.generation !== lastHistoryGeneration.current) {
    tacticalPlan.current = emptyMovementPlan();
    lastHistoryGeneration.current = history?.generation;
  }

  useEffect(() => {
    if (historyBusy) setProductionQueue(null);
  }, [historyBusy]);

  // A build may open payment and placement decisions before the next production offer.
  // Keep the queue above the workflow renderer and resume only on a fresh legal offer.
  useEffect(() => {
    if (
      !productionQueue?.units.length ||
      !choice ||
      productionSubmitting.current ||
      choice.nonce === submittedNonce.current
    )
      return;
    if (choice.context?.subtype !== "produce_unit") {
      if (choice.context?.subtype !== "pay_resources" && choice.context?.subtype !== "place_unit") {
        setProductionError("Production ended; remaining staged builds were not submitted.");
        setProductionQueue(null);
      }
      return;
    }
    const system =
      choice.context.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    if (choice.actor !== productionQueue.actor || system !== productionQueue.system) {
      setProductionError("Production changed; remaining staged builds were not submitted.");
      setProductionQueue(null);
      return;
    }
    const unit = productionQueue.units[0];
    const matching = choice.options.filter(
      (candidate) =>
        candidate.kind !== "decline" && (candidate.payload?.unit === unit || candidate.id === unit),
    );
    if (matching.length !== 1) {
      setProductionError(
        `${unit} is no longer uniquely offered; remaining staged builds were not submitted.`,
      );
      setProductionQueue(null);
      return;
    }
    const option = matching[0];
    submittedNonce.current = choice.nonce;
    productionSubmitting.current = true;
    // Submit a single authoritative build at a time. Its response includes the
    // next decision, so payment can safely interrupt before we resume the queue.
    const submit = onSubmitBasketBatch
      ? onSubmitBasketBatch({
          kind: "production",
          destination: system,
          steps: [{ kind: "produce", unit, count: Number(option.payload?.count ?? 1) }],
        })
      : onSubmitChoice(option.id);
    void submit
      .then(() => {
        productionSubmitting.current = false;
        setProductionQueue((current) =>
          current && current.actor === choice.actor && current.system === system
            ? { ...current, units: current.units.slice(1) }
            : current,
        );
      })
      .catch((error: unknown) => {
        productionSubmitting.current = false;
        setProductionError(error instanceof Error ? error.message : String(error));
        setProductionQueue(null);
        submittedNonce.current = null;
      });
  }, [choice, productionQueue, onSubmitChoice, onSubmitBasketBatch]);

  const playersMap = React.useMemo<Record<string, PlayerView>>(() => {
    if (!players) return {};
    if (Array.isArray(players)) {
      return Object.fromEntries(players.map((p) => [p.id, p]));
    }
    return players;
  }, [players]);

  useEffect(() => {
    // Only automatically un-minimize if it's the viewer's turn to make a decision
    if (!choice || viewerSeat === undefined || choice.actor === viewerSeat) {
      setIsChoiceMinimized(false);
    }
  }, [choice?.nonce, choice?.actor, viewerSeat]);

  // Register open drawer in overlayStack for Escape dismissal
  useEffect(() => {
    if (!openDrawer) return;
    const unregister = overlayStack.register({
      id: `drawer-${openDrawer}`,
      modal: false,
      onDismiss: () => setOpenDrawer(null),
    });
    return unregister;
  }, [openDrawer]);

  return (
    <div data-testid="game-container" className="app-shell">
      <div className="app-shell__header">{header}</div>
      <div className="app-shell__content">
        <main className="app-shell__board">{board}</main>
        <aside
          id="player-sheet-drawer"
          data-testid="player-sheet-drawer"
          className={`app-shell__player-sheet${openDrawer === "players" ? " app-shell__drawer--open" : ""}`}
        >
          {playerSheet}
        </aside>
      </div>

      {detail}

      <div className="app-shell__mobile-actions" aria-label="Game panels">
        <button
          type="button"
          data-testid="player-sheet-toggle"
          className="button button--secondary"
          aria-expanded={openDrawer === "players"}
          aria-controls="player-sheet-drawer"
          onClick={() => setOpenDrawer((drawer) => (drawer === "players" ? null : "players"))}
        >
          Players
        </button>
        <button
          type="button"
          data-testid="event-log-mobile-toggle"
          className="button button--secondary"
          aria-expanded={openDrawer === "events"}
          aria-controls="event-log-drawer"
          onClick={() => setOpenDrawer((drawer) => (drawer === "events" ? null : "events"))}
        >
          Events
        </button>
      </div>

      <section
        id="event-log-drawer"
        className={`app-shell__event-log${openDrawer === "events" ? " app-shell__drawer--open" : ""}`}
        aria-label="Event log"
      >
        <EventLog
          events={events}
          currentPath={currentPath}
          historyKey={logHistoryKey}
          cursor={history?.cursor}
          redoCount={history?.redo_count}
          busy={historyBusy}
          onRestore={
            onChangeHistory
              ? (cursor) => onChangeHistory({ cursor }, (history?.cursor ?? 0) - cursor)
              : undefined
          }
          onChangeHistory={onChangeHistory ? (action) => onChangeHistory(action) : undefined}
          isOpen={openDrawer === "events"}
          onToggle={() => setOpenDrawer((drawer) => (drawer === "events" ? null : "events"))}
        />
      </section>

      <div className="app-shell__overlays">
        <PipelineRunnerContext.Provider value={pipelineRunner}>
          <ChoiceRendererDispatcher
            key={`${history?.generation ?? 0}:${boardView?.invasion?.invasion_seq ?? "none"}`}
            choice={choice}
            viewerSeat={viewerSeat}
            players={playersMap}
            boardView={boardView}
            activeSystemId={activeSystemId}
            revealedObjectives={revealedObjectives}
            scoredObjectives={scoredObjectives}
            objectiveProgress={objectiveProgress}
            onSubmit={onSubmitChoice}
            onSubmitMovementBatch={onSubmitMovementBatch}
            onSubmitBasketBatch={onSubmitBasketBatch}
            lastError={lastError}
            selectedOptionId={selectedOptionId}
            selectedSystemId={selectedSystemId}
            onSelectOption={onSelectOption}
            isMinimized={isChoiceMinimized}
            onMinimizedChange={setIsChoiceMinimized}
            tacticalPlan={tacticalPlan}
            tacticalStep={tacticalStep}
            onTacticalStep={() => setTacticalStep((step) => step + 1)}
            productionQueue={productionQueue?.units}
            productionError={productionError}
            landingDraft={
              landingKey && landingDraftState?.key === landingKey ? landingDraftState.entries : []
            }
            onLandingDraftChange={(entries) => {
              if (landingKey) setLandingDraftState({ key: landingKey, entries });
            }}
            onQueueProduction={(units) => {
              if (!choice || productionQueue?.units.length) return;
              const system =
                choice.context?.target && "System" in choice.context.target
                  ? choice.context.target.System
                  : "";
              setProductionError(null);
              submittedNonce.current = null;
              setProductionQueue({ actor: choice.actor, system, units });
            }}
          />
        </PipelineRunnerContext.Provider>
      </div>
    </div>
  );
};
