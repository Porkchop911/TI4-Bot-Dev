import React, { useState, useEffect, useRef, useMemo } from "react";
import { PendingChoiceDto } from "../protocol/types.ts";
import { Dialog } from "../primitives/index.ts";
import { usePipelineRunner, SemanticIntent } from "../hooks/usePipelineRunner.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { findStrategyCardMeta } from "../protocol/contentCatalog.ts";
import { DecisionHeader } from "./DecisionHeader.tsx";

export interface PendingChoiceModalProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  onSubmit: (optionId: string) => Promise<void>;
  lastError?: string | null;
  isMinimized?: boolean;
  onMinimizedChange?: (isMinimized: boolean) => void;
  selectedOptionId?: string;
  onSelectOption?: (optionId: string) => void;
  selectedOptionIds?: string[];
  onSelectOptions?: (optionIds: string[]) => void;
}

export const PendingChoiceModal: React.FC<PendingChoiceModalProps> = ({
  choice,
  model,
  onSubmit,
  lastError,
  isMinimized: controlledIsMinimized,
  onMinimizedChange,
  selectedOptionId: controlledSelectedOptionId,
  onSelectOption,
  selectedOptionIds: controlledSelectedOptionIds,
  onSelectOptions,
}) => {
  const present = useParticipantText();
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
  const setIsMinimized = (next: boolean) => {
    if (controlledIsMinimized === undefined) setUncontrolledIsMinimized(next);
    onMinimizedChange?.(next);
  };

  const constraints = model?.outstanding?.[0] ?? choice?.context?.outstanding?.[0];
  const minSelection =
    model?.selectionMode.mode === "multi"
      ? model.selectionMode.min
      : (constraints?.min_selection ?? 1);
  const maxSelection =
    model?.selectionMode.mode === "multi"
      ? model.selectionMode.max
      : (constraints?.max_selection ??
        (constraints?.min_selection ? constraints.min_selection : 1));
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
    return (
      opt.label.toLowerCase().includes(q) || (opt.description?.toLowerCase().includes(q) ?? false)
    );
  });
  const strategyDraft = choice.context?.subtype === "draft_strategy_card";

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

  // Minimized floating banner allowing inspection of map, players, and tables
  if (isMinimized) {
    return (
      <div data-testid="minimized-choice-banner" className="choice-banner panel">
        <span>{choice.prompt}</span>
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
            {choice.prompt}
          </Dialog.Title>
          <DecisionHeader
            actor={choice.actor}
            title={choice.prompt}
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
          {choice.options.length >= 6 && (
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
                      <div>
                        <div style={{ fontWeight: 600, color: isChecked ? "#38bdf8" : "#e2e8f0" }}>
                          {card ? `${card.initiative}. ${card.name}` : opt.label}
                        </div>
                        {card && (
                          <div className="strategy-draft-card__text">
                            <strong>Primary</strong>
                            <p>
                              {card.primaryText || opt.description || "No printed text available."}
                            </p>
                            <strong>Secondary</strong>
                            <p>{card.secondaryText || "No printed text available."}</p>
                          </div>
                        )}
                        {opt.description && (
                          <div
                            style={{
                              fontSize: 12,
                              color: "#94a3b8",
                              marginTop: 2,
                              whiteSpace: "pre-line",
                            }}
                          >
                            {opt.description}
                          </div>
                        )}
                      </div>
                    </label>
                  );
                })
              )}
            </div>

            <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 8 }}>
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
                  : strategyDraft
                    ? "Choose card"
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
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
