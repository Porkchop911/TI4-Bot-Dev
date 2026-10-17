import React, { useState, useEffect, useRef, useMemo } from "react";
import { BoardView, ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { Dialog } from "../primitives/index.ts";
import { usePipelineRunner, SemanticIntent } from "../hooks/usePipelineRunner.ts";
import { ChoiceRendererModel, isDeclineOption } from "../presentation/choiceModel.ts";
import { useParticipantText, useIsSeat, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { findStrategyCardMeta } from "../protocol/contentCatalog.ts";
import {
  describeCardOption,
  handDecisionConfirmLabel,
  handDecisionNote,
} from "../presentation/cardOptions.ts";
import { describeStrategySecondary } from "../presentation/strategySecondary.ts";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { describeCommandTokens, gainTokensPrompt, type TokenOutcome } from "../presentation/commandTokens.ts";
import { CommandTokenPanel } from "./CommandTokenPanel.tsx";
import { TokenPaymentBar } from "./TokenPaymentBar.tsx";
import { StrategySecondaryPanel } from "./StrategySecondaryPanel.tsx";
import { describeTradeReplenish } from "../presentation/tradeReplenish.ts";
import { TradeReplenishPanel } from "./TradeReplenishPanel.tsx";
import { SystemPickConfirmBar, SystemPickMapButton, SystemPickOptionFacts } from "./SystemPickParts.tsx";
import { RemoveUnitPanel, RemoveUnitOptionNote } from "./RemoveUnitParts.tsx";
import { UnitAbilityOptionNote } from "./UnitAbilityParts.tsx";
import { describeUnitAbilityOption } from "../presentation/unitAbilityOptions.ts";
import { describeRemoveUnit } from "../presentation/removeUnit.ts";
import { PoliticsContextPanel, PoliticsOptionNote } from "./PoliticsDecisionParts.tsx";
import { LegendaryContextPanel, LegendaryOptionNote } from "./LegendaryParts.tsx";
import { NotOfferedNote } from "./NotOfferedNote.tsx";
import { InvestmentsContextPanel, StrategyGoodsBadge, StrategyGoodsNote } from "./StrategyGoodsParts.tsx";
import { describeAbilityOffer } from "../presentation/abilityOffer.ts";
import { UnitPickOptionNote, UnitPickPanel } from "./UnitPickParts.tsx";
import { TechnologyPickOptionNote, TechnologyPickPanel } from "./TechnologyPickParts.tsx";
import { PredictOutcomeNote, PredictOutcomePanel } from "./PredictOutcomeParts.tsx";
import { ExploreRewardPanel } from "./ExploreRewardParts.tsx";
import { CommodityStandingPanel } from "./CommodityStandingFacts.tsx";
import { describeExploreCard } from "../presentation/exploreReward.ts";
import { AbilityOfferPanel } from "./AbilityOfferPanel.tsx";
import { describeOfferCard } from "../presentation/offerCard.ts";
import { OfferCardPanel } from "./OfferCardPanel.tsx";
import { describeVoteGoods } from "../presentation/voteGoods.ts";
import { usePlanetResources } from "../presentation/PlanetResourcesContext.tsx";
import { VoteGoodsPanel } from "./VoteGoodsPanel.tsx";
import { useAgendaPill } from "./AgendaStrip.tsx";
import { investmentsProgress, isStrategyCardGrid } from "../presentation/strategyGoods.ts";
import { pickOptionTitle } from "../presentation/pickTitles.ts";
import { describeStrategyCardPick } from "../presentation/strategyCardPick.ts";
import { describeRerollDice, rerollIntents } from "../presentation/rerollDice.ts";
import { describeGroundMover } from "../presentation/groundMover.ts";
import { describeNumberStepper } from "../presentation/numberStepper.ts";
import { RerollDicePanel } from "./RerollDicePanel.tsx";
import { GroundMoverPanel } from "./GroundMoverPanel.tsx";
import { NumberStepperPanel } from "./NumberStepperPanel.tsx";
import { LawPickOptionNote, LawPickPanel } from "./LawPickParts.tsx";
import { StrategyCardHolderTag, StrategyCardPickPanel } from "./StrategyCardPickParts.tsx";

export interface PendingChoiceModalProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  onSubmit: (optionId: string) => Promise<void>;
  /** Sends a staged plan as one server batch; without it a token gain stays a plain list. */
  onSubmitBatch?: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
  lastError?: string | null;
  isMinimized?: boolean;
  onMinimizedChange?: (isMinimized: boolean) => void;
  selectedOptionId?: string;
  onSelectOption?: (optionId: string) => void;
  selectedOptionIds?: string[];
  onSelectOptions?: (optionIds: string[]) => void;
  /** Lets decisions that name units and systems show what is there. */
  boardView?: BoardView;
}

export const PendingChoiceModal: React.FC<PendingChoiceModalProps> = ({
  choice,
  model,
  onSubmit,
  onSubmitBatch,
  lastError,
  isMinimized: controlledIsMinimized,
  onMinimizedChange,
  selectedOptionId: controlledSelectedOptionId,
  onSelectOption,
  selectedOptionIds: controlledSelectedOptionIds,
  onSelectOptions,
  boardView,
}) => {
  const presentText = useParticipantText();
  const isSeat = useIsSeat();
  const seatName = usePlayerIdentity();
  /** A player option ("elect a") names the player rather than the raw seat id. */
  const present = (text: string) => presentText(text);
  const presentOption = (opt: { id: string; kind?: string; label: string }) =>
    (opt.kind === "player" || opt.kind === "vote") && isSeat(opt.id)
      ? opt.label.replace(
          new RegExp(`(^|\\s)${opt.id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?=$|\\s)`),
          (_m, lead: string) => `${lead}${seatName(opt.id).label}`,
        )
      : present(opt.label);
  const planetResources = usePlanetResources();
  const [uncontrolledSelectedOptionId, setUncontrolledSelectedOptionId] = useState<string>("");
  const [uncontrolledSelectedOptionIds, setUncontrolledSelectedOptionIds] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);
  const [uncontrolledIsMinimized, setUncontrolledIsMinimized] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const priorFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    setSubmissionError(null);
  }, [choice?.nonce]);

  const {
    executePipeline,
    isRunning: isPipelineRunning,
    lastError: pipelineError,
  } = usePipelineRunner(choice, onSubmit);

  const isMinimized = controlledIsMinimized ?? uncontrolledIsMinimized;
  const agendaPill = useAgendaPill();
  const setIsMinimized = (next: boolean) => {
    if (controlledIsMinimized === undefined) setUncontrolledIsMinimized(next);
    onMinimizedChange?.(next);
  };

  const minSelection = model?.selectionMode.mode === "multi" ? model.selectionMode.min : 1;
  const maxSelection = model?.selectionMode.mode === "multi" ? model.selectionMode.max : 1;
  const isMultiSelect = maxSelection > 1;

  const selectedOptionId = controlledSelectedOptionId ?? uncontrolledSelectedOptionId;
  const setSelectedOptionId = (id: string) => {
    if (controlledSelectedOptionId === undefined) setUncontrolledSelectedOptionId(id);
    onSelectOption?.(id);
  };

  const selectedOptionIds = useMemo(() => {
    return controlledSelectedOptionIds ?? uncontrolledSelectedOptionIds;
  }, [controlledSelectedOptionIds, uncontrolledSelectedOptionIds]);

  const setSelectedOptionIds = (ids: string[]) => {
    if (controlledSelectedOptionIds === undefined) setUncontrolledSelectedOptionIds(ids);
    onSelectOptions?.(ids);
  };

  // Auto-select initial state when new choice arrives
  useEffect(() => {
    if (!choice || choice.options.length === 0) return;

    if (!isMultiSelect) {
      if (controlledSelectedOptionId === undefined) {
        setUncontrolledSelectedOptionId(choice.options[0].id);
      }
    } else {
      if (controlledSelectedOptionIds === undefined) {
        setUncontrolledSelectedOptionIds([]);
      }
    }
    setSearchQuery("");
    setIsSubmitting(false);
    setIsMinimized(false);
  }, [choice?.nonce, isMultiSelect]);

  useEffect(() => {
    if (!choice || isMinimized) return;
    priorFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialogRef.current?.focus();
    return () => priorFocusRef.current?.focus();
  }, [choice?.nonce, isMinimized]);

  if (!choice) return null;

  // Filtered options based on search query
  const filteredOptions = choice.options.filter((opt) => {
    if (!searchQuery.trim()) return true;
    const q = searchQuery.toLowerCase();
    return opt.label.toLowerCase().includes(q);
  });
  const strategyDraft = isStrategyCardGrid(choice);
  const investments = investmentsProgress(choice);

  const handleToggleOption = (id: string) => {
    if (isMultiSelect) {
      if (selectedOptionIds.includes(id)) {
        const next = selectedOptionIds.filter((item) => item !== id);
        setSelectedOptionIds(next);
      } else {
        if (selectedOptionIds.length < maxSelection) {
          const next = [...selectedOptionIds, id];
          setSelectedOptionIds(next);
        }
      }
    } else {
      setSelectedOptionId(id);
    }
  };

  const confirmTokens = async (outcome: TokenOutcome) => {
    if (outcome.kind === "option") await onSubmit(outcome.optionId);
    else if (outcome.kind === "moves") {
      // One engine question per move; the pipeline answers them in turn.
      const intents: SemanticIntent[] = outcome.moveIds.map((id) => ({
        predicate: (o: ChoiceOption) => o.id === id,
      }));
      if (outcome.finish) intents.push({ predicate: isDeclineOption });
      executePipeline(intents);
    } else await onSubmitBatch?.({ kind: "tokens", steps: outcome.steps });
  };
  // Minimized floating banner allowing inspection of map, players, and tables
  if (isMinimized) {
    // Leadership's purchase is paid by clicking planets on the map; this bar confirms it.
    const minimizedTokens = describeCommandTokens(choice, Boolean(onSubmitBatch), planetResources);
    if (minimizedTokens?.purchase) {
      return (
        <TokenPaymentBar
          view={minimizedTokens}
          onConfirm={confirmTokens}
          onOpenPanel={() => setIsMinimized(false)}
        />
      );
    }
    return (
      <div data-testid="minimized-choice-banner" className="choice-banner panel">
        <span className={agendaPill ? "agenda-pill" : undefined}>{agendaPill ?? gainTokensPrompt(choice)}</span>
        <SystemPickConfirmBar
          choice={choice}
          board={boardView}
          selectedOptionId={selectedOptionId}
          onSubmit={onSubmit}
        />
        <button
          type="button"
          data-testid="resume-choice-button"
          onClick={() => setIsMinimized(false)}
          className="button button--primary"
        >
          Resume decision
        </button>
      </div>
    );
  }

  const isSelectionValid = isMultiSelect
    ? selectedOptionIds.length >= minSelection && selectedOptionIds.length <= maxSelection
    : choice.options.some((opt) => opt.id === selectedOptionId);

  // Leadership's secondary window plans its purchase on the token panel instead.
  const tokens = describeCommandTokens(choice, Boolean(onSubmitBatch), planetResources);
  const secondary = tokens ? null : describeStrategySecondary(choice);
  const replenish = tokens || secondary ? null : describeTradeReplenish(choice);
  const abilityOffer = tokens || secondary || replenish ? null : describeAbilityOffer(choice);
  const offerCard =
    tokens || secondary || replenish || abilityOffer ? null : describeOfferCard(choice);
  const voteGoods =
    tokens || secondary || replenish || abilityOffer || offerCard ? null : describeVoteGoods(choice);
  // Action cards with a picker of their own, in place of one radio per answer.
  const hasOwnPanel = Boolean(tokens || secondary || replenish || abilityOffer || offerCard || voteGoods);
  const reroll = hasOwnPanel ? null : describeRerollDice(choice);
  const mover = hasOwnPanel ? null : describeGroundMover(choice, boardView);
  const stepper = hasOwnPanel ? null : describeNumberStepper(choice);
  const cardPicker = reroll ?? mover ?? stepper;
  // The engine asks about one die at a time; the panel covers the whole roll.
  const rerollTitle = `${reroll?.source ?? "Reroll"}: reroll any number of your dice`;
  const strategyPick = describeStrategyCardPick(choice);
  // Overrule and Strategize leave the card where it is: trade goods on it are not collected.
  const showGoods = !strategyPick || strategyPick.kind === "disgrace";
  const submitOption = async (optionId: string) => {
    if (isSubmitting || isPipelineRunning) return;
    setSubmissionError(null);
    setIsSubmitting(true);
    try {
      await onSubmit(optionId);
    } catch (error) {
      setSubmissionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isSelectionValid || isSubmitting || isPipelineRunning) return;
    setSubmissionError(null);

    if (isMultiSelect) {
      const intents: SemanticIntent[] = selectedOptionIds
        .filter((id) => choice.options.some((opt) => opt.id === id))
        .map((optId) => ({
          predicate: (o) => o.id === optId,
        }));
      executePipeline(intents);
    } else {
      setIsSubmitting(true);
      try {
        await onSubmit(selectedOptionId);
      } catch (error) {
        setSubmissionError(error instanceof Error ? error.message : String(error));
      } finally {
        setIsSubmitting(false);
      }
    }
  };

  return (
    <Dialog.Root open={!isMinimized} onOpenChange={(open) => setIsMinimized(!open)}>
      <Dialog.Content
        ref={dialogRef}
        data-testid="pending-choice-dialog"
        className="choice-dialog"
        onEscape={() => setIsMinimized(true)}
        // Same as every other panel: a tap on the dimmed area outside the window closes it.
        onClick={(e) => {
          if (e.target === e.currentTarget) setIsMinimized(true);
        }}
        initialFocusRef={dialogRef}
        style={{
          background: "rgba(3, 7, 18, 0.75)",
          backdropFilter: "blur(4px)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <div
          className="choice-dialog__panel panel"
          style={{
            padding: 24,
            maxWidth: strategyDraft ? 1100 : 960,
            width: "95%",
            display: "flex",
            flexDirection: "column",
            gap: 16,
          }}
        >
          <Dialog.Title as="h2" className="visually-hidden">
            {reroll ? rerollTitle : gainTokensPrompt(choice)}
          </Dialog.Title>
          <DecisionHeader
            actor={choice.actor}
            title={reroll ? rerollTitle : gainTokensPrompt(choice)}
            progress={present(handDecisionNote(choice) ?? "") || undefined}
            choice={choice}
            onMinimize={() => setIsMinimized(true)}
            titleTestId="choice-prompt"
            minimizeTestId="minimize-choice-button"
          />

          {/* Bounded Multi-Select Status Indicator */}
          {isMultiSelect && (
            <div
              data-testid="multi-selection-badge"
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: selectedOptionIds.length >= minSelection ? "#4ade80" : "#facc15",
                background: "rgba(30, 41, 59, 0.8)",
                padding: "4px 10px",
                borderRadius: 4,
                border: "1px solid #334155",
                display: "inline-block",
              }}
            >
              Selected: {selectedOptionIds.length} of {maxSelection} (Minimum: {minSelection})
            </div>
          )}

          {/* Search Bar for long option lists */}
          {choice.options.length >= 6 && !tokens && (
            <input
              type="search"
              data-testid="choice-search-input"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search options..."
              aria-label="Filter options"
              style={{
                background: "#0f172a",
                border: "1px solid #334155",
                borderRadius: 6,
                padding: "6px 12px",
                color: "#f8fafc",
                fontSize: 13,
              }}
            />
          )}

          {/* Error banner if rejected */}
          {(lastError || submissionError || pipelineError) && (
            <div
              data-testid="choice-error-banner"
              role="alert"
              style={{
                background: "rgba(239, 68, 68, 0.15)",
                border: "1px solid #ef4444",
                color: "#fca5a5",
                padding: "8px 12px",
                borderRadius: 6,
                fontSize: 13,
              }}
            >
              {present(lastError || submissionError || pipelineError || "")}
            </div>
          )}

          {secondary && (
            <StrategySecondaryPanel
              view={secondary}
              disabled={isSubmitting || isPipelineRunning}
              onChoose={(id) => void submitOption(id)}
            />
          )}
          {replenish && (
            <TradeReplenishPanel
              view={replenish}
              disabled={isSubmitting || isPipelineRunning}
              onChoose={(id) => void submitOption(id)}
            />
          )}
          {abilityOffer && (
            <AbilityOfferPanel
              view={abilityOffer}
              disabled={isSubmitting || isPipelineRunning}
              onChoose={(id) => void submitOption(id)}
            />
          )}
          {offerCard && (
            <OfferCardPanel
              view={offerCard}
              disabled={isSubmitting || isPipelineRunning}
              onChoose={(id) => void submitOption(id)}
            />
          )}
          {reroll && (
            <RerollDicePanel
              key={reroll.dice.map((die) => `${die.unit}${die.face}`).join("-")}
              view={reroll}
              disabled={isSubmitting || isPipelineRunning}
              onConfirm={(ticked) =>
                executePipeline(rerollIntents(reroll, ticked).map((predicate) => ({ predicate })))
              }
            />
          )}
          {mover && (
            <GroundMoverPanel
              view={mover}
              disabled={isSubmitting || isPipelineRunning}
              onChoose={(id) => void submitOption(id)}
            />
          )}
          {stepper && (
            <NumberStepperPanel
              view={stepper}
              disabled={isSubmitting || isPipelineRunning}
              onConfirm={(id) => void submitOption(id)}
            />
          )}
          {voteGoods && (
            <VoteGoodsPanel
              key={choice.nonce}
              view={voteGoods}
              disabled={isSubmitting || isPipelineRunning}
              onChoose={(id) => void submitOption(id)}
            />
          )}
          {tokens && (
            <CommandTokenPanel
              key={choice.nonce}
              view={tokens}
              disabled={isSubmitting || isPipelineRunning}
              onConfirm={confirmTokens}
              onPayOnMap={() => setIsMinimized(true)}
            />
          )}
          {!secondary && !tokens && <RemoveUnitPanel choice={choice} board={boardView} />}
          {!secondary && !tokens && !replenish && <PoliticsContextPanel choice={choice} />}
          {!secondary && !tokens && !replenish && <LegendaryContextPanel choice={choice} />}
          {!secondary && !tokens && !replenish && <InvestmentsContextPanel choice={choice} />}
          <NotOfferedNote choice={choice} />
          {!secondary && !tokens && !replenish && <UnitPickPanel choice={choice} />}
          {!secondary && !tokens && !replenish && <LawPickPanel choice={choice} />}
          {!secondary && !tokens && !replenish && <StrategyCardPickPanel choice={choice} />}
          {!secondary && !tokens && !replenish && <TechnologyPickPanel choice={choice} />}
          {!secondary && !tokens && !replenish && <PredictOutcomePanel choice={choice} />}
          {!secondary && !tokens && !replenish && !offerCard && <ExploreRewardPanel choice={choice} />}
          {!secondary && !tokens && !replenish && !offerCard && !describeExploreCard(choice) && (
            <CommodityStandingPanel choice={choice} />
          )}
          {!secondary && !tokens && !replenish && !abilityOffer && !offerCard && !voteGoods && !cardPicker && (
          <form
            onSubmit={handleSubmit}
            style={{ display: "flex", flexDirection: "column", gap: 12 }}
          >
            {isMultiSelect && selectedOptionIds.length > 0 && (
              <>
                <button
                  type="button"
                  className="button button--secondary"
                  onClick={() => setSelectedOptionIds([])}
                >
                  Reset selection
                </button>
                <p className="text-muted">
                  Confirm submits one choice at a time. Later options must be offered again; an
                  interrupted sequence stops.
                </p>
              </>
            )}
            <div
              className={strategyDraft ? "strategy-draft-grid" : undefined}
              style={{
                display: strategyDraft ? "grid" : "flex",
                flexDirection: strategyDraft ? undefined : "column",
                gap: 8,
                maxHeight: "min(60dvh, 650px)",
                overflowY: "auto",
              }}
            >
              {filteredOptions.length === 0 ? (
                <div style={{ padding: 16, textAlign: "center", color: "#94a3b8", fontSize: 13 }}>
                  No matching options found.
                </div>
              ) : (
                filteredOptions.map((opt) => {
                  const isChecked = isMultiSelect
                    ? selectedOptionIds.includes(opt.id)
                    : selectedOptionId === opt.id;
                  const isMaxReached =
                    isMultiSelect && selectedOptionIds.length >= maxSelection && !isChecked;
                  const card = strategyDraft ? findStrategyCardMeta(opt.id) : null;
                  const handCard = describeCardOption(choice.context?.subtype, opt);

                  return (
                    <label
                      key={opt.id}
                      data-testid="choice-option"
                      data-option-id={opt.id}
                      data-actionable={!isMaxReached}
                      className={`${strategyDraft ? "strategy-draft-card " : ""}card${isChecked ? " card--selected" : ""}`}
                      style={{
                        display: "flex",
                        alignItems: "flex-start",
                        gap: 10,
                        padding: "10px 14px",
                        cursor: isMaxReached ? "not-allowed" : "pointer",
                        opacity: isMaxReached ? 0.5 : 1,
                        fontSize: 14,
                        transition: "all 0.15s ease",
                      }}
                    >
                      <input
                        type={isMultiSelect ? "checkbox" : "radio"}
                        name="choice-option"
                        value={opt.id}
                        checked={isChecked}
                        onChange={() => handleToggleOption(opt.id)}
                        disabled={isSubmitting || isPipelineRunning || isMaxReached}
                        style={{ marginTop: 3 }}
                        aria-checked={isChecked}
                      />
                      <div style={{ flex: "1 1 auto", minWidth: 0 }}>
                        <div
                          className={card ? "strategy-draft-card__title" : undefined}
                          style={{ fontWeight: 600, color: isChecked ? "#38bdf8" : "#e2e8f0" }}
                        >
                          <span>
                            {card ? `${card.initiative}. ${card.name}` : (handCard?.title ?? pickOptionTitle(choice, opt, boardView) ?? describeRemoveUnit(choice, boardView)?.option(opt).title ?? describeUnitAbilityOption(choice, opt, boardView)?.title ?? presentOption(opt))}
                          </span>
                          {card && showGoods && <StrategyGoodsBadge option={opt} />}
                        </div>
                        {card && <StrategyCardHolderTag choice={choice} option={opt} />}
                        {handCard?.badge && (
                          <div className="card-option__badge" data-testid="card-option-badge">
                            {handCard.badge}
                          </div>
                        )}
                        {handCard?.text && (
                          <p className="card-option__text" data-testid="card-option-text">
                            {handCard.text}
                          </p>
                        )}
                        {card && (
                          <div className="strategy-draft-card__text">
                            <strong>Primary</strong>
                            {strategyPick?.half === "primary" && (
                              <span className="strategy-performed" data-testid="strategy-performed">
                                {" "}performed
                              </span>
                            )}
                            <p>
                              {card.primaryText || "No printed text available."}
                            </p>
                            <strong>Secondary</strong>
                            {strategyPick?.half === "secondary" && (
                              <span className="strategy-performed" data-testid="strategy-performed">
                                {" "}performed
                              </span>
                            )}
                            <p>{card.secondaryText || "No printed text available."}</p>
                          </div>
                        )}
                        <UnitAbilityOptionNote choice={choice} option={opt} board={boardView} />
                        <RemoveUnitOptionNote choice={choice} option={opt} board={boardView} />
                        <UnitPickOptionNote choice={choice} option={opt} board={boardView} />
                        <LawPickOptionNote choice={choice} option={opt} />
                        <TechnologyPickOptionNote choice={choice} option={opt} />
                        <PredictOutcomeNote choice={choice} option={opt} />
                        <PoliticsOptionNote choice={choice} option={opt} />
                        <LegendaryOptionNote choice={choice} option={opt} board={boardView} />
                        {card && showGoods && <StrategyGoodsNote choice={choice} option={opt} />}
                        <SystemPickOptionFacts choice={choice} optionId={opt.id} board={boardView} />
                      </div>
                    </label>
                  );
                })
              )}
            </div>

            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 8 }}>
              <SystemPickMapButton
                choice={choice}
                board={boardView}
                onMinimize={() => setIsMinimized(true)}
              />
              <button
                type="submit"
                data-testid="submit-choice-button"
                disabled={!isSelectionValid || isSubmitting || isPipelineRunning}
                className="button button--primary"
                style={{
                  padding: "10px 20px",
                  background:
                    isSelectionValid && !isSubmitting && !isPipelineRunning ? undefined : "#475569",
                  transition: "background 0.15s ease",
                }}
              >
                {isSubmitting || isPipelineRunning
                  ? "Submitting..."
                  : investments
                    ? "Place trade good"
                    : strategyDraft
                    ? "Choose card"
                    : handDecisionConfirmLabel(choice.context?.subtype)
                      ? handDecisionConfirmLabel(choice.context?.subtype)
                      : choice.context?.subtype === "activate_system"
                      ? "Activate system"
                      : choice.context?.subtype === "commit_ground_forces"
                        ? "Land forces"
                        : choice.context?.subtype?.includes("score_")
                          ? "Confirm score"
                          : "Confirm choice"}
              </button>
            </div>
          </form>
          )}
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
