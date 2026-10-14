import { act, useState } from "react";
import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import { useDeclineOption, useNonceReset } from "./useWorkflowState.ts";
import { PendingChoiceDto } from "../protocol/types.ts";

const choice: PendingChoiceDto = {
  actor: "seat_1",
  nonce: "nonce-1",
  prompt: "Choose",
  context: { subtype: "" },
  options: [
    { id: "continue", label: "Continue" },
    { id: "not-now", label: "Not now", kind: "decline" },
  ],
};

describe("workflow state hooks", () => {
  it("finds decline options by id or kind", () => {
    const { result } = renderHook(() => useDeclineOption(choice));
    expect(result.current?.id).toBe("not-now");
  });

  it("resets local workflow state only when the choice nonce changes", () => {
    const { result, rerender } = renderHook(
      ({ nonce }) => {
        const [value, setValue] = useState("draft");
        useNonceReset(nonce, () => setValue("reset"));
        return { value, setValue };
      },
      { initialProps: { nonce: "nonce-1" } },
    );

    act(() => {
      result.current.setValue("edited");
    });
    rerender({ nonce: "nonce-1" });
    expect(result.current.value).toBe("edited");

    rerender({ nonce: "nonce-2" });
    expect(result.current.value).toBe("reset");
  });
});
