import { gainTokensPrompt } from "../presentation/commandTokens.ts";
import { optionUnit, produceStep } from "../presentation/productionDraft.ts";
import React, { useEffect, useState, useMemo, useRef } from "react";
import { GameLogEntry } from "../hooks/useGameSession.ts";
import {
  BoardView,
  TacticalFacts,
  CurrentLogPath,
  HistoryStatus,
  PendingChoiceDto,
  PlayerView,
  ObjectiveProgressView,
} from "../protocol/types.ts";
import { EventLog } from "./EventLog.tsx";
import { NotificationLogButton } from "./NotificationLog.tsx";
import { TurnRedoBar } from "./TurnRedoBar.tsx";
import type { TurnRedoRequestControls } from "./EventLog.tsx";
import type { TurnRedoBusy } from "../hooks/useTurnRedo.ts";
import type { TurnRedoStatus } from "../protocol/turnRedo.ts";
import { BoardPrepSlotContext } from "../presentation/BoardPrepSlot.tsx";
import { PausedPlanBanner } from "./PausedPlanBanner.tsx";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { TechnologyModal } from "./TechnologyModal.tsx";
import { FeedbackDockButton, useFeedbackDock } from "../feedback/FeedbackDockButton.tsx";
import { ObjectivesModal } from "./ObjectivesModal.tsx";
import { PaymentDrawer } from "./PaymentDrawer.tsx";
import { PaymentBar } from "./PaymentBar.tsx";
import {
  TacticalMovementOverlay,
  emptyMovementPlan,
  type ExecutionPlan,
} from "./TacticalMovementOverlay.tsx";
import { SpaceCombatOverlay } from "./CombatResolutionModal.tsx";
import { TradeDeskModal } from "./TradeDeskModal.tsx";
import { AgendaBallotModal } from "./AgendaBallotModal.tsx";
import { AgendaPillProvider, AgendaStrip, useAgendaPill } from "./AgendaStrip.tsx";
import { ActiveEffectsStrip } from "./ActiveEffectsStrip.tsx";
import type { AgendaStrip as AgendaStripModel } from "../presentation/agendaStrip.ts";
import { ReactionStatusBar } from "./ReactionStatusBar.tsx";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";
import { TradeGoodLedgerProvider } from "../presentation/TradeGoodLedgerContext.tsx";
import { CargoLoadingTray } from "./CargoLoadingTray.tsx";
import { InvasionLandingTray } from "./InvasionLandingTray.tsx";
import type { Landing } from "./InvasionLandingTray.tsx";
import { InvasionOverlay } from "./InvasionOverlay.tsx";
import { SystemActivationBar } from "./SystemActivationBar.tsx";
import { PlanetSelectionBar } from "./PlanetSelectionBar.tsx";
import { ReadyPlanetsBar } from "./ReadyPlanetsBar.tsx";
import { TurnActionBar } from "./TurnActionBar.tsx";
import {
  deriveReadOnlyTurnBar,
  deriveTurnBar,
  isTurnMenuChoice,
} from "../presentation/turnBar.ts";
import {
  deriveChoiceRendererModel,
  ChoiceRendererModel,
  isPlanetSelectionChoice,
} from "../presentation/choiceModel.ts";
import { Dialog, overlayStack } from "../primitives/index.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { useLoneAutoSubmit } from "../hooks/useLoneAutoSubmit.ts";
import { readSinglePayment } from "../hooks/useSinglePaymentSetting.ts";
import {
  ProductionPaymentProvider,
  useProductionPayment,
  type ProductionPaymentApi,
} from "../presentation/ProductionPaymentContext.tsx";
import {
  autoPaymentFor,
  isProductionPayQuestion,
  newProductionPay,
  type ProductionPay,
} from "../presentation/productionPayment.ts";
import {
  PipelineRunnerContext,
  useOwnedPipelineRunner,
} from "../hooks/usePipelineRunner.ts";

export interface GameShellProps {
  header: React.ReactNode;
  board: React.ReactNode;
  playerSheet: React.ReactNode;
  detail?: React.ReactNode;
  /** Phone bottom buttons: opens the Objectives dialog (absent: no Objectives button). */
  onOpenObjectives?: () => void;
  /** The Objectives window is open: on a phone it is a sheet like the drawers (toggled by its button). */
  objectivesOpen?: boolean;
  onCloseObjectives?: () => void;
  events: GameLogEntry[];
  history?: HistoryStatus;
  currentPath?: CurrentLogPath;
  logHistoryKey?: unknown;
  onChangeHistory?: (
    action: import("../protocol/client.ts").HistoryChange,
    steps?: number,
  ) => void;
  historyBusy?: boolean;
  /** Single-player undo: takes back the viewer's last answer; resolves true when it worked. Absent: no button. */
  onUndoLast?: () => Promise<boolean>;
  /** Single-player games list the event log newest first. */
  newestFirst?: boolean;
  /** Told when the client answered a lone strategic action / activation, for the corner toast. */
  onAutoSubmitNotice?: (note: { id: string; text: string; prompt?: string }) => void;
  /** Loads the replay JSON for the event log's "Copy replay" button (seated players). */
  onFetchReplay?: () => Promise<{ text: string; filename: string }>;
  /** Turn redo (seated players): the status strip and the "Redo my last turn" controls. */
  turnRedo?: TurnRedoShellProps;
  choice: PendingChoiceDto | null;
  /** The agenda being decided, for the strip under the top bar and the minimized pill; null outside the agenda phase. */
  agenda?: AgendaStripModel | null;
  /** Lingering action card effects in force now (`GameView.active_effects`), for the strip under the top bar. */
  activeEffects?: import("../protocol/types.ts").ActiveEffectDto[];
  /** What the minimized decision pill says about the agenda ("Voting: Minister of Peace — For/Against"). */
  agendaPill?: string | null;
  /** With `nonce` (the question that was shown) a stale answer is dropped instead of sent. */
  onSubmitChoice: (optionId: string, nonce?: string) => Promise<void>;
  /** The viewing seat's "never offer" cards (server state) and how to change them. */
  reactionModes?: import("../protocol/types.ts").ReactionModes;
  onSetReactionMode?: (
    card: string,
    mode: import("../protocol/types.ts").ReactionModeSetting,
  ) => void;
  onSubmitMovementBatch?: (
    destination: string,
    steps: import("../protocol/client.ts").MovementStep[],
  ) => Promise<void>;
  onSubmitBasketBatch?: (
    plan: import("../protocol/client.ts").BasketPlan,
  ) => Promise<void>;
  /** A plan the server paused at a reaction window, with how to continue or drop it. */
  batchResume?: import("../protocol/client.ts").BatchResume | null;
  onResumeBatch?: () => Promise<void>;
  onDismissBatchResume?: () => void;
  lastError?: string | null;
  selectedOptionId?: string;
  selectedSystemId?: string | null;
  onSelectOption?: (optionId: string) => void;
  selectedPlanetId?: string | null;
  onSelectPlanet?: (planetId: string | null) => void;
  /** Highlights a system on the map (reaction dialogs link the system involved). */
  onShowSystem?: (systemId: string) => void;
  viewerSeat?: string | null;
  players?: Record<string, PlayerView> | PlayerView[];
  boardView?: BoardView;
  /** The engine's facts for the pending tactical choice (routes and loads of the ships). */
  tacticalFacts?: TacticalFacts | null;
  activeSystemId?: string | null;
  revealedObjectives?: readonly string[];
  scoredObjectives?: Record<string, string[]>;
  objectiveProgress?: Record<string, Record<string, ObjectiveProgressView>>;
  homeSystemLost?: Record<string, string[]>;
  productionQueue?: readonly string[];
  productionError?: string | null;
  onQueueProduction?: (units: string[]) => void;
  /** The phase and whose turn it is, for the read-only action bar when it is not your turn. */
  turn?: TurnInfo;
  /** The game, for keeping a prepared strategy-card secondary apart per game (device storage). */
  /** Secondary preparation chrome (chip, banner, prepared-answer bar, toasts), rendered over the board. */
  prepOverlay?: React.ReactNode;
  /**
   * Filled with the production builder's own "Confirm builds" (one batch for a single build, the
   * build queue for several), so a prepared Warfare secondary is sent exactly the way a click is.
   */
  productionSubmitRef?: React.MutableRefObject<
    ((units: string[], destination: string, real: PendingChoiceDto) => Promise<void>) | null
  >;
}

export interface TurnRedoShellProps {
  status: TurnRedoStatus | null;
  busy: TurnRedoBusy;
  error: string | null;
  onRequest: TurnRedoRequestControls["onRequest"];
  onAutoplay: () => void;
  onRestore: () => void;
  onKeep: () => void;
  /** Host only: the seats whose turn may be redone. */
  seats?: TurnRedoRequestControls["seats"];
  viewerSeat?: string | null;
  /** A game on this device: the button reads "Undo my last turn". */
  local?: boolean;
}

export interface TurnInfo {
  phase: string;
  activePlayer: string | null;
  /** The game has ended: no turn bar, not even the read-only one. */
  over?: boolean;
  /** The game waits for another seat's decision or for a reaction window (the turn status). */
  othersDeciding?: boolean;
}

export interface ChoiceRendererDispatcherProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string, nonce?: string) => Promise<void>;
  reactionModes?: GameShellProps["reactionModes"];
  onSetReactionMode?: GameShellProps["onSetReactionMode"];
  /** A decision the client is answering for the seat; its dialog stays closed (movement, nothing movable). */
  autoAnswerNonce?: string | null;
  onSubmitMovementBatch?: GameShellProps["onSubmitMovementBatch"];
  onSubmitBasketBatch?: GameShellProps["onSubmitBasketBatch"];
  lastError?: string | null;
  selectedOptionId?: string;
  selectedSystemId?: string | null;
  onSelectOption?: (optionId: string) => void;
  selectedPlanetId?: string | null;
  onSelectPlanet?: (planetId: string | null) => void;
  onShowSystem?: (systemId: string) => void;
  isMinimized: boolean;
  onMinimizedChange: (minimized: boolean) => void;
  players?: Record<string, PlayerView>;
  boardView?: BoardView;
  /** The engine's facts for the pending tactical choice (routes and loads of the ships). */
  tacticalFacts?: TacticalFacts | null;
  activeSystemId?: string | null;
  revealedObjectives?: readonly string[];
  scoredObjectives?: Record<string, string[]>;
  objectiveProgress?: Record<string, Record<string, ObjectiveProgressView>>;
  homeSystemLost?: Record<string, string[]>;
  productionQueue?: readonly string[];
  productionError?: string | null;
  onQueueProduction?: (units: string[]) => void;
  tacticalPlan?: React.RefObject<ExecutionPlan>;
  tacticalStep?: number;
  onTacticalStep?: () => void;
  landingDraft?: Landing[];
  onLandingDraftChange?: (draft: Landing[]) => void;
  turn?: TurnInfo;
  /** The public log, for reaction dialogs that carry no trigger of their own. */
  events?: GameLogEntry[];
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
  tacticalFacts,
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
    facts={tacticalFacts?.kind === "movement" ? tacticalFacts : null}
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

const renderPlanetSelection: WorkflowRenderer = ({
  choice,
  model,
  viewerSeat,
  selectedOptionId,
  selectedPlanetId,
  onSelectOption,
  onSelectPlanet,
  onSubmit,
  boardView,
  lastError,
}) => (
  <PlanetSelectionBar
    choice={choice}
    model={model}
    viewerSeat={viewerSeat}
    selectedOptionId={selectedOptionId}
    selectedPlanetId={selectedPlanetId}
    onSelectOption={onSelectOption}
    onSelectPlanet={onSelectPlanet}
    onSubmit={onSubmit}
    boardView={boardView}
    lastError={lastError}
  />
);

// Diplomacy: every exhausted planet at once, one Confirm; without a batch channel the old
// one-planet-at-a-time bar answers the same questions.
const renderReadyPlanets: WorkflowRenderer = (props) =>
  props.onSubmitBasketBatch ? (
    <ReadyPlanetsBar
      choice={props.choice}
      viewerSeat={props.viewerSeat}
      boardView={props.boardView}
      onConfirm={props.onSubmitBasketBatch}
      lastError={props.lastError}
    />
  ) : (
    renderPlanetSelection(props)
  );

const workflowRenderers = new Map<
  ChoiceRendererModel["workflow"],
  WorkflowRenderer
>([
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
  ["ready_planets", renderReadyPlanets],
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
    ({
      choice,
      model,
      viewerSeat,
      onSubmit,
      isMinimized,
      onMinimizedChange,
      lastError,
      events,
      boardView,
      activeSystemId,
      turn,
      players,
      onShowSystem,
      reactionModes,
      onSetReactionMode,
    }) => (
      <ReactionStatusBar
        players={players}
        onShowSystem={onShowSystem}
        reactionModes={reactionModes}
        onSetReactionMode={onSetReactionMode}
        choice={choice}
        model={model}
        viewerSeat={viewerSeat}
        events={events}
        boardView={boardView}
        activeSystemId={activeSystemId}
        activePlayerId={turn?.activePlayer ?? null}
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
      players,
    }) => (
      <ProductionBuilderDrawer
        choice={choice}
        model={model}
        viewerSeat={viewerSeat}
        tradeGoods={players?.[choice.actor]?.trade_goods}
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
  ["planet_selection", renderPlanetSelection],
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
  homeSystemLost,
  selectedOptionId,
  onSelectOption,
  boardView,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <ObjectivesModal
      isOpen={!isMinimized}
      onClose={() => onMinimizedChange(true)}
      revealedObjectives={revealedObjectives}
      scoredObjectives={scoredObjectives}
      objectiveProgress={objectiveProgress}
      homeSystemLost={homeSystemLost}
      players={players}
      viewerSeat={viewerSeat}
      choice={choice}
      model={model}
      onSubmit={onSubmit}
      lastError={lastError}
      selectedOptionId={selectedOptionId}
      onSelectOption={onSelectOption}
      isScoringMode={true}
      board={boardView}
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
  onSubmitBasketBatch,
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
      onSubmitBatch={onSubmitBasketBatch}
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
  players,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <TradeDeskModal
      players={players}
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
  onSelectOption,
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
      onSelectOption={onSelectOption}
    />
  );
}

function renderGeneric({
  choice,
  model,
  onSubmit,
  onSubmitBasketBatch,
  lastError,
  isMinimized,
  onMinimizedChange,
  selectedOptionId,
  onSelectOption,
  boardView,
}: Parameters<WorkflowRenderer>[0]) {
  return (
    <PendingChoiceModal
      boardView={boardView}
      choice={choice}
      model={model}
      onSubmit={onSubmit}
      onSubmitBatch={onSubmitBasketBatch}
      lastError={lastError}
      isMinimized={isMinimized}
      onMinimizedChange={onMinimizedChange}
      selectedOptionId={selectedOptionId}
      onSelectOption={onSelectOption}
    />
  );
}

export const ChoiceRendererDispatcher: React.FC<
  ChoiceRendererDispatcherProps
> = ({
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
  selectedPlanetId,
  onSelectPlanet,
  isMinimized,
  onMinimizedChange,
  players,
  boardView,
  tacticalFacts,
  activeSystemId,
  revealedObjectives,
  scoredObjectives,
  objectiveProgress,
  homeSystemLost,
  productionQueue,
  productionError,
  onQueueProduction,
  tacticalPlan,
  tacticalStep,
  onTacticalStep,
  landingDraft,
  onLandingDraftChange,
  turn,
  events,
  onShowSystem,
  reactionModes,
  onSetReactionMode,
  autoAnswerNonce,
}) => {
  const present = useParticipantText();
  const agendaPill = useAgendaPill();
  const derivedModel = useMemo(() => {
    return choice
      ? deriveChoiceRendererModel(choice, viewerSeat ?? null)
      : null;
  }, [choice, viewerSeat]);

  const productionPay = useProductionPayment()?.plan ?? null;
  const model = propModel ?? derivedModel;
  const workflow = model?.workflow ?? "generic_selection";
  const combatSubtype = choice?.context?.subtype;
  const spectatorCombatWorkflow =
    combatSubtype === "sustain_damage" ||
    combatSubtype === "assign_casualty" ||
    combatSubtype === "assault_cannon_destroy" ||
    combatSubtype === "courageous_to_the_end_assign_casualty" ||
    combatSubtype === "announce_retreat" ||
    combatSubtype === "retreat_to";
  const isBattleChoice = Boolean(
    boardView?.combat && choice?.context?.space_battle,
  );

  const isCombatWorkflow =
    spectatorCombatWorkflow ||
    isBattleChoice ||
    workflow === "combat_sustain" ||
    workflow === "combat_casualty" ||
    workflow === "combat_retreat";

  // Paying during an invasion (the custodians' six influence, a Dunlain Reaper's two resources,
  // Freelancers' unit) is an ordinary payment: the payer gets the payment panel and the map's
  // payable planets with both values and the running total, not a list of "exhaust" buttons.
  const invasionPayment =
    workflow === "payment" && Boolean(choice) && choice?.actor === viewerSeat;
  if (boardView?.invasion && !boardView.combat && !invasionPayment) {
    // Participant ids in the prompt and option labels ("player_<hash>'s units" for a
    // bombardment target) are shown as the seat's name, as in every other decision.
    const shownChoice = choice
      ? {
          ...choice,
          prompt: present(choice.prompt),
          options: choice.options.map((option) => ({
            ...option,
            label: present(option.label),
          })),
        }
      : choice;
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
        choice={shownChoice}
        players={players}
        viewerSeat={viewerSeat}
        onSubmit={onSubmit}
        onSubmitBatch={onSubmitBasketBatch}
        onClose={() => onMinimizedChange(true)}
        lastError={lastError}
        landingDraft={landingDraft}
        onLandingDraftChange={onLandingDraftChange}
        othersDeciding={turn?.othersDeciding ?? false}
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

  // Other seats see a slim waiting bar for a planet pick, so the pending action stays visible
  // while the map remains usable.
  if (choice && choice.actor !== viewerSeat && isPlanetSelectionChoice(choice))
    return (
      <PlanetSelectionBar
        choice={{ ...choice, prompt: present(choice.prompt) }}
        viewerSeat={viewerSeat}
        onSubmit={onSubmit}
        boardView={boardView}
      />
    );

  // Not your turn in the action phase: the same bar, read-only, with the reason on every button.
  const viewer = viewerSeat ? players?.[viewerSeat] : undefined;
  const readOnlyBar =
    viewer && turn?.phase === "action" && !turn.over && !isCombatWorkflow ? (
      <TurnActionBar
        model={deriveReadOnlyTurnBar(viewer, turn.activePlayer)}
        onSubmit={onSubmit}
      />
    ) : null;

  if (
    !choice ||
    (viewerSeat !== undefined &&
      choice.actor !== viewerSeat &&
      !isCombatWorkflow)
  )
    return readOnlyBar;

  // Movement with no unit that can move is finished for the player (see useLoneAutoSubmit): no dialog.
  if (choice.nonce === autoAnswerNonce && choice.prompt === "movement") return null;

  // A production's later payments are answered from the plan the player confirmed: no prompt.
  if (
    productionPay?.mode === "auto" &&
    choice.nonce !== productionPay.panelNonce &&
    isProductionPayQuestion(choice, productionPay)
  )
    return (
      <div
        className="choice-banner panel system-activation-bar"
        role="status"
        data-testid="production-pay-auto"
      >
        Paying for the remaining builds from your plan…
      </div>
    );

  // A view-only copy: IDs, payloads and the original pending choice stay intact.
  const visibleChoice = {
    ...choice,
    prompt: present(gainTokensPrompt(choice)),
    options: choice.options.map((option) => ({
      ...option,
      label: present(option.label),
    })),
  };

  // The turn menu and the end-turn question live on the persistent bar. Options the bar cannot
  // place keep the old list.
  if (isTurnMenuChoice(visibleChoice) && choice.actor === viewerSeat) {
    const barModel = deriveTurnBar(visibleChoice, viewer);
    if (barModel)
      return (
        <TurnActionBar
          model={barModel}
          // A click on a bar that is still painted after its question was answered (the next
          // question is already pending) must not answer the next one: the nonce says which.
          onSubmit={(optionId) => onSubmit(optionId, choice.nonce)}
          lastError={lastError ? present(lastError) : lastError}
        />
      );
  }

  const renderer =
    isBattleChoice || (spectatorCombatWorkflow && !model)
      ? renderCombat
      : (workflowRenderers.get(workflow) ??
        workflowRenderers.get("generic_selection")!);
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
    selectedPlanetId,
    onSelectPlanet,
    isMinimized,
    onMinimizedChange,
    players,
    boardView,
    tacticalFacts,
    activeSystemId,
    revealedObjectives,
    scoredObjectives,
    objectiveProgress,
    homeSystemLost,
    productionQueue,
    productionError,
    onQueueProduction,
    tacticalPlan,
    tacticalStep,
    onTacticalStep,
    turn,
    events,
    onShowSystem,
    reactionModes,
    onSetReactionMode,
  });

  return (
    <>
      {/* Minimized Decision Pill for dedicated drawers/modals */}
      {isMinimized &&
        ![
          "generic_selection",
          "strategy_card_draft",
          "system_activation",
          "planet_selection",
          "ready_planets",
          "payment",
          "combat_sustain",
          "combat_casualty",
          "combat_retreat",
        ].includes(workflow) && (
          <div
            className="choice-banner choice-minimized-pill"
            data-testid="choice-minimized-pill"
          >
            <span className={`choice-minimized-pill__prompt${agendaPill ? " agenda-pill" : ""}`}>
              {workflow === "technology_research"
                ? "Research Technology"
                : workflow === "objective_scoring"
                  ? "Score Objective"
                  : workflow === "tactical_movement" ||
                      visibleChoice.prompt === "movement"
                    ? "Move Units"
                    : workflow === "tactical_cargo" ||
                        visibleChoice.prompt === "load_cargo"
                      ? "Load Cargo"
                      : (agendaPill ?? visibleChoice.prompt)}
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

      {/* While the payment list is minimised the map is the control; this bar confirms. */}
      {isMinimized && workflow === "payment" && (
        <PaymentBar
          choice={visibleChoice}
          model={model}
          viewerSeat={viewerSeat}
          player={players?.[choice.actor] ?? null}
          onSubmit={onSubmit}
          onSubmitBatch={onSubmitBasketBatch}
          onOpenList={() => onMinimizedChange(false)}
          lastError={lastError ? present(lastError) : lastError}
        />
      )}

      {wrappedWorkflow ? (
        <Dialog.Root
          open={!isMinimized}
          onOpenChange={(open) => onMinimizedChange(!open)}
        >
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
  onOpenObjectives,
  objectivesOpen,
  onCloseObjectives,
  events,
  history,
  currentPath,
  logHistoryKey,
  onChangeHistory,
  historyBusy,
  onUndoLast,
  newestFirst,
  onAutoSubmitNotice,
  onFetchReplay,
  turnRedo,
  choice,
  agenda,
  activeEffects,
  agendaPill,
  onSubmitChoice,
  onSubmitMovementBatch,
  onSubmitBasketBatch,
  batchResume,
  onResumeBatch,
  onDismissBatchResume,
  lastError,
  selectedOptionId,
  selectedSystemId,
  onSelectOption,
  selectedPlanetId,
  onSelectPlanet,
  onShowSystem,
  reactionModes,
  onSetReactionMode,
  viewerSeat,
  players,
  boardView,
  tacticalFacts,
  activeSystemId,
  revealedObjectives,
  scoredObjectives,
  objectiveProgress,
  homeSystemLost,
  turn,
  prepOverlay,
  productionSubmitRef,
}) => {
  const [openDrawer, setOpenDrawer] = useState<"events" | "players" | null>(
    null,
  );
  const [isChoiceMinimized, setIsChoiceMinimized] = useState(false);
  // One payment for a whole production; null when the builds are paid one question at a time.
  const [productionPay, setProductionPay] = useState<ProductionPay | null>(null);
  const [productionQueue, setProductionQueueState] = useState<{
    actor: string;
    system: string;
    units: string[];
  } | null>(null);
  const [productionError, setProductionError] = useState<string | null>(null);
  // Ending the queue ends its payment plan too.
  const setProductionQueue = React.useCallback<typeof setProductionQueueState>((value) => {
    setProductionQueueState(value);
    if (value === null) setProductionPay(null);
  }, []);
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
  const { answeringNonce } = useLoneAutoSubmit({
    choice,
    viewerSeat,
    history,
    busy: pipelineRunner.isRunning || Boolean(historyBusy),
    submit: onSubmitChoice,
    onNotice: onAutoSubmitNotice,
  });

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
      // A reaction window (or another seat's decision) comes in the middle of a production and
      // hands it back: wait for it instead of dropping the remaining builds and their payment.
      if (
        choice.context?.subtype?.startsWith("reaction_") ||
        choice.actor !== productionQueue.actor
      )
        return;
      if (
        choice.context?.subtype !== "pay_resources" &&
        choice.context?.subtype !== "place_unit"
      ) {
        setProductionError(
          "Production ended; remaining staged builds were not submitted.",
        );
        setProductionQueue(null);
      }
      return;
    }
    const system =
      choice.context.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    if (
      choice.actor !== productionQueue.actor ||
      system !== productionQueue.system
    ) {
      setProductionError(
        "Production changed; remaining staged builds were not submitted.",
      );
      setProductionQueue(null);
      return;
    }
    const unit = productionQueue.units[0];
    const matching = choice.options.filter(
      (candidate) => candidate.kind !== "decline" && optionUnit(candidate) === unit,
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
    setProductionPay((plan) => (plan ? { ...plan, submitted: plan.submitted + 1 } : plan));
    // Submit a single authoritative build at a time. Its response includes the
    // next decision, so payment can safely interrupt before we resume the queue.
    const submit = onSubmitBasketBatch
      ? onSubmitBasketBatch({
          kind: "production",
          destination: system,
          steps: [
            produceStep(option),
          ],
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
        setProductionError(
          error instanceof Error ? error.message : String(error),
        );
        setProductionQueue(null);
        submittedNonce.current = null;
      });
  }, [choice, productionQueue, onSubmitChoice, onSubmitBasketBatch]);

  // A prepared production is sent through the same two paths the builder's confirm uses.
  useEffect(() => {
    if (!productionSubmitRef) return;
    // `real` is the engine's own decision: while an auto-played answer is held back the shell is
    // shown no decision at all.
    productionSubmitRef.current = async (units, destination, real) => {
      if (real.context?.subtype !== "produce_unit" || !units.length) {
        throw new Error("The production decision is no longer open");
      }
      const system =
        real.context.target && "System" in real.context.target
          ? real.context.target.System
          : destination;
      if (units.length === 1 && onSubmitBasketBatch) {
        const option = real.options.find(
          (candidate) => candidate.kind !== "decline" && optionUnit(candidate) === units[0],
        );
        if (!option) throw new Error(`${units[0]} is no longer offered`);
        await onSubmitBasketBatch({
          kind: "production",
          destination: system,
          steps: [produceStep(option)],
        });
        return;
      }
      if (productionQueue?.units.length) throw new Error("A production queue is already running");
      setProductionError(null);
      submittedNonce.current = null;
      setProductionPay(readSinglePayment() ? newProductionPay(real, units) : null);
      setProductionQueueState({ actor: real.actor, system, units });
    };
    return () => {
      productionSubmitRef.current = null;
    };
  }, [productionSubmitRef, onSubmitBasketBatch, productionQueue]);

  const playersMap = React.useMemo<Record<string, PlayerView>>(() => {
    if (!players) return {};
    if (Array.isArray(players)) {
      return Object.fromEntries(players.map((p) => [p.id, p]));
    }
    return players;
  }, [players]);

  // The production's one payment: the first payment question is the player's panel; the later
  // ones are answered from what is left of that plan. Anything the plan does not predict stops it
  // and the normal payment is asked (suggestion prefilled); reaction windows and other seats'
  // decisions only pause it, so it resumes at the next payment question of this production.
  useEffect(() => {
    const plan = productionPay;
    if (!plan || !choice) return;
    // The last build is placed and the engine offers production again: the production is over.
    if (
      choice.actor === plan.actor &&
      choice.context?.subtype === "produce_unit" &&
      !productionQueue?.units.length &&
      !productionSubmitting.current &&
      choice.nonce !== submittedNonce.current &&
      plan.submitted >= plan.costs.length
    ) {
      setProductionPay(null);
      return;
    }
    if (plan.mode !== "auto" || choice.nonce === plan.handledNonce) return;
    if (viewerSeat !== undefined && viewerSeat !== null && choice.actor !== viewerSeat) return;
    if (choice.actor !== plan.actor || choice.context?.subtype !== "pay_resources") return;
    const stop = (note: string) =>
      setProductionPay((current) =>
        current ? { ...current, mode: "manual", handledNonce: choice.nonce, note } : current,
      );
    if (!isProductionPayQuestion(choice, plan)) {
      stop("This payment is not part of the plan you confirmed, so it is asked on its own.");
      return;
    }
    const owner = playersMap[choice.actor];
    const auto = autoPaymentFor(
      choice,
      plan.remaining,
      owner?.trade_goods ?? plan.remaining.tradeGoods,
    );
    if (!auto) {
      stop(
        plan.remaining.planetIds.length || plan.remaining.tradeGoods
          ? "The planets and trade goods you planned no longer fit what this build asks. Choose how to pay for it."
          : "The planned payment is used up. Choose how to pay for this build.",
      );
      return;
    }
    const payload = { kind: "payment" as const, steps: auto.steps };
    if (!onSubmitBasketBatch && auto.steps.length !== 1) {
      stop("This payment needs several steps; choose how to pay for it.");
      return;
    }
    // Spent from the plan before the answer lands, so the next question sees what is left.
    setProductionPay({ ...plan, remaining: auto.rest, handledNonce: choice.nonce, note: undefined });
    const answer = onSubmitBasketBatch
      ? onSubmitBasketBatch(payload)
      : onSubmitChoice(
          auto.steps[0].kind === "trade_good" ? "trade_good" : `exhaust|${auto.steps[0].planet}`,
        );
    void answer
      .then(() =>
        onAutoSubmitNotice?.({
          id: `production-pay-${choice.nonce}`,
          text: auto.summary,
          prompt: "production payment plan",
        }),
      )
      .catch((error: unknown) =>
        setProductionPay((current) =>
          current
            ? {
                ...current,
                mode: "manual",
                handledNonce: choice.nonce,
                note: `Automatic payment stopped: ${error instanceof Error ? error.message : String(error)}`,
              }
            : current,
        ),
      );
  }, [choice, productionPay, productionQueue, playersMap, viewerSeat, onSubmitBasketBatch, onSubmitChoice, onAutoSubmitNotice]);

  const productionPayApi = useMemo<ProductionPaymentApi>(
    () => ({
      plan: productionPay,
      confirm: (rest, nonce) =>
        setProductionPay((current) =>
          current
            ? { ...current, mode: "auto", remaining: rest, panelNonce: nonce, handledNonce: nonce, note: undefined }
            : current,
        ),
      revert: () =>
        setProductionPay((current) =>
          current ? { ...current, mode: "ask", remaining: { planetIds: [], tradeGoods: 0 }, panelNonce: undefined, handledNonce: undefined } : current,
        ),
      askEach: (note) =>
        setProductionPay((current) =>
          current ? { ...current, mode: "manual", handledNonce: undefined, note } : current,
        ),
    }),
    [productionPay],
  );

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

  // Like the dialogs: a tap outside an open phone drawer closes it. The drawers and the panel buttons
  // (which toggle them) are left alone, and so is anything stacked above (a dialog or a card).
  useEffect(() => {
    if (!openDrawer && !objectivesOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      // Only where the drawers overlay the game (phones); on a desktop the log is a docked panel.
      if (!window.matchMedia?.("(max-width: 720px)").matches) return;
      const target = event.target as Element | null;
      if (!target?.closest) return;
      if (
        target.closest(
          "#player-sheet-drawer, #event-log-drawer, #objectives-drawer, .app-shell__mobile-actions, [role='dialog'], .detail-panel, .corner-toast-layer",
        )
      )
        return;
      setOpenDrawer(null);
      onCloseObjectives?.();
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => window.removeEventListener("pointerdown", onPointerDown, true);
  }, [openDrawer, objectivesOpen, onCloseObjectives]);

  const feedbackInDock = useFeedbackDock();

  const [prepSlot, setPrepSlot] = useState<HTMLElement | null>(null);
  const prepSlotValue = useMemo(() => ({ slot: prepSlot, setSlot: setPrepSlot }), [prepSlot]);

  return (
    <BoardPrepSlotContext.Provider value={prepSlotValue}>
    <AgendaPillProvider value={agendaPill ?? null}>
    <TradeGoodLedgerProvider players={Object.values(playersMap)} events={events}>
    <div data-testid="game-container" className="app-shell">
      <div className="app-shell__header">
        {header}
        {agenda && <AgendaStrip strip={agenda} />}
        <ActiveEffectsStrip effects={activeEffects} viewerSeat={viewerSeat} />
      </div>
      <div className="app-shell__content">
        <main className="app-shell__board">
          {board}
          {/* In the board column (not the whole shell), so its offset is measured from the board. */}
          {prepOverlay}
        </main>
        <aside
          id="player-sheet-drawer"
          data-testid="player-sheet-drawer"
          className={`app-shell__player-sheet${openDrawer === "players" ? " app-shell__drawer--open" : ""}`}
        >
          <button
            type="button"
            data-testid="player-sheet-close"
            className="button button--secondary app-shell__drawer-close"
            onClick={() => setOpenDrawer(null)}
          >
            Close
          </button>
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
          onClick={() => {
            onCloseObjectives?.();
            setOpenDrawer((drawer) => (drawer === "players" ? null : "players"));
          }}
        >
          Players
        </button>
        {onOpenObjectives && (
          <button
            type="button"
            data-testid="objectives-mobile-button"
            className="button button--secondary"
            aria-expanded={Boolean(objectivesOpen)}
            aria-controls="objectives-drawer"
            onClick={() => {
              setOpenDrawer(null);
              // Pressing it again closes the sheet, like Players / Events.
              if (objectivesOpen && onCloseObjectives) onCloseObjectives();
              else onOpenObjectives();
            }}
          >
            Objectives
          </button>
        )}
        <NotificationLogButton testId="toast-history-mobile-toggle" />
        <button
          type="button"
          data-testid="event-log-mobile-toggle"
          className="button button--secondary"
          aria-expanded={openDrawer === "events"}
          aria-controls="event-log-drawer"
          onClick={() => {
            onCloseObjectives?.();
            setOpenDrawer((drawer) => (drawer === "events" ? null : "events"));
          }}
        >
          Events
        </button>
        {feedbackInDock && <FeedbackDockButton />}
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
              ? (cursor) =>
                  onChangeHistory({ cursor }, (history?.cursor ?? 0) - cursor)
              : undefined
          }
          onChangeHistory={
            onChangeHistory ? (action) => onChangeHistory(action) : undefined
          }
          onFetchReplay={onFetchReplay}
          newestFirst={newestFirst}
          undoLast={
            onUndoLast
              ? { available: history?.can_undo === true, busy: historyBusy ?? false, onUndo: onUndoLast }
              : undefined
          }
          turnRedo={
            turnRedo
              ? {
                  onRequest: turnRedo.onRequest,
                  disabled: turnRedo.busy !== null || turnRedo.status !== null,
                  seats: turnRedo.seats,
                  viewerSeat: turnRedo.viewerSeat,
                  local: turnRedo.local,
                }
              : undefined
          }
          isOpen={openDrawer === "events"}
          onToggle={() =>
            setOpenDrawer((drawer) => (drawer === "events" ? null : "events"))
          }
        />
      </section>

      <div className="app-shell__overlays">
        {turnRedo && (
          <TurnRedoBar
            status={turnRedo.status}
            busy={turnRedo.busy}
            error={turnRedo.error}
            onAutoplay={turnRedo.onAutoplay}
            onRestore={turnRedo.onRestore}
            onKeep={turnRedo.onKeep}
          />
        )}
        {batchResume && onResumeBatch && onDismissBatchResume && (
          <PausedPlanBanner
            resume={batchResume}
            choice={choice}
            viewerSeat={viewerSeat}
            onContinue={onResumeBatch}
            onDismiss={onDismissBatchResume}
          />
        )}
        <ProductionPaymentProvider value={productionPayApi}>
        <PipelineRunnerContext.Provider value={pipelineRunner}>
          <ChoiceRendererDispatcher
            key={`${history?.generation ?? 0}:${boardView?.invasion?.invasion_seq ?? "none"}`}
            choice={choice}
            autoAnswerNonce={answeringNonce}
            viewerSeat={viewerSeat}
            players={playersMap}
            boardView={boardView}
            tacticalFacts={tacticalFacts}
            activeSystemId={activeSystemId}
            revealedObjectives={revealedObjectives}
            scoredObjectives={scoredObjectives}
            objectiveProgress={objectiveProgress}
            homeSystemLost={homeSystemLost}
            turn={turn}
            events={events}
            onSubmit={onSubmitChoice}
            onSubmitMovementBatch={onSubmitMovementBatch}
            onSubmitBasketBatch={onSubmitBasketBatch}
            lastError={lastError}
            selectedOptionId={selectedOptionId}
            selectedSystemId={selectedSystemId}
            onSelectOption={onSelectOption}
            selectedPlanetId={selectedPlanetId}
            onSelectPlanet={onSelectPlanet}
            onShowSystem={onShowSystem}
            reactionModes={reactionModes}
            onSetReactionMode={onSetReactionMode}
            isMinimized={isChoiceMinimized}
            onMinimizedChange={setIsChoiceMinimized}
            tacticalPlan={tacticalPlan}
            tacticalStep={tacticalStep}
            onTacticalStep={() => setTacticalStep((step) => step + 1)}
            productionQueue={productionQueue?.units}
            productionError={productionError}
            landingDraft={
              landingKey && landingDraftState?.key === landingKey
                ? landingDraftState.entries
                : []
            }
            onLandingDraftChange={(entries) => {
              if (landingKey)
                setLandingDraftState({ key: landingKey, entries });
            }}
            onQueueProduction={(units) => {
              if (!choice || productionQueue?.units.length) return;
              const system =
                choice.context?.target && "System" in choice.context.target
                  ? choice.context.target.System
                  : "";
              setProductionError(null);
              submittedNonce.current = null;
              setProductionPay(readSinglePayment() ? newProductionPay(choice, units) : null);
              setProductionQueueState({ actor: choice.actor, system, units });
            }}
          />
        </PipelineRunnerContext.Provider>
        </ProductionPaymentProvider>
      </div>
    </div>
    </TradeGoodLedgerProvider>
    </AgendaPillProvider>
    </BoardPrepSlotContext.Provider>
  );
};
