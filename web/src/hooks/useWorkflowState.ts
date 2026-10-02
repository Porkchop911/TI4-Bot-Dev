import { useEffect, useMemo } from "react";
import { PendingChoiceDto } from "../protocol/types.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";

export const useDeclineOption = (
  choice: PendingChoiceDto | null,
  model?: ChoiceRendererModel | null,
) =>
  useMemo(
    () =>
      model?.declineOption ??
      choice?.options.find((option) => option.id === "decline" || option.kind === "decline") ??
      null,
    [choice, model],
  );

export const useNonceReset = (nonce: string | undefined, reset: () => void) => {
  useEffect(() => {
    reset();
  }, [nonce]);
};
