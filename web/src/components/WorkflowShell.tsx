import React, { useState } from "react";
import { PendingChoiceDto } from "../protocol/types.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { useDeclineOption, useNonceReset } from "../hooks/useWorkflowState.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";

export interface WorkflowShellProps {
  choice: PendingChoiceDto;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  lastError?: string | null;
  spectatorNotice?: React.ReactNode;
  spectatorNoticeTestId?: string;
  errorTestId?: string;
  children: (state: WorkflowShellState) => React.ReactNode;
}

export interface WorkflowShellState {
  isActor: boolean;
  isDirectSubmitting: boolean;
  declineOption: PendingChoiceDto["options"][number] | null;
  submitDirect: (optionId: string) => Promise<void>;
}

export const WorkflowShell: React.FC<WorkflowShellProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  lastError,
  spectatorNotice,
  spectatorNoticeTestId,
  errorTestId,
  children,
}) => {
  const present = useParticipantText();
  const [isDirectSubmitting, setIsDirectSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);
  const isActor = !viewerSeat || choice.actor === viewerSeat;
  const declineOption = useDeclineOption(choice, model);
  useNonceReset(choice.nonce, () => {
    setIsDirectSubmitting(false);
    setSubmissionError(null);
  });

  const submitDirect = async (optionId: string) => {
    if (isDirectSubmitting) return;
    setSubmissionError(null);
    setIsDirectSubmitting(true);
    try {
      await onSubmit(optionId);
    } catch (error) {
      setSubmissionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsDirectSubmitting(false);
    }
  };

  return (
    <>
      {!isActor && spectatorNotice && (
        <div data-testid={spectatorNoticeTestId} role="status">
          {spectatorNotice}
        </div>
      )}
      {children({ isActor, isDirectSubmitting, declineOption, submitDirect })}
      {(lastError || submissionError) && (
        <div data-testid={errorTestId} role="alert">
          {present(lastError || submissionError || "")}
        </div>
      )}
    </>
  );
};
