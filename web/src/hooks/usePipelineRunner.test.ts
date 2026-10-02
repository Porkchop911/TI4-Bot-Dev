import { describe, it, expect, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { usePipelineRunner, SemanticIntent } from "./usePipelineRunner.ts";
import { PendingChoiceDto } from "../protocol/types.ts";

describe("usePipelineRunner", () => {
  it("executes sequential intents and resets isRunning to false on completion", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const initialChoice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Pay debt",
      options: [
        { id: "exhaust|jord", label: "Jord" },
        { id: "exhaust|mecatol", label: "Mecatol Rex" },
      ],
    };

    let currentChoice: PendingChoiceDto | null = initialChoice;

    const { result, rerender } = renderHook(({ choice }) => usePipelineRunner(choice, onSubmit), {
      initialProps: { choice: currentChoice },
    });

    expect(result.current.isRunning).toBe(false);

    const intents: SemanticIntent[] = [
      {
        predicate: (opt) => opt.id === "exhaust|jord",
      },
      {
        predicate: (opt) => opt.id === "exhaust|mecatol",
      },
    ];

    // Start pipeline
    await act(async () => {
      result.current.executePipeline(intents);
    });

    expect(onSubmit).toHaveBeenCalledWith("exhaust|jord");

    // Simulate choice update from server after step 1
    currentChoice = {
      actor: "p1",
      nonce: "n2",
      prompt: "Pay remaining debt",
      options: [{ id: "exhaust|mecatol", label: "Mecatol Rex" }],
    };

    await act(async () => {
      rerender({ choice: currentChoice });
    });

    expect(onSubmit).toHaveBeenCalledWith("exhaust|mecatol");

    // After all intents complete, isRunning MUST be false (no lockout)
    expect(result.current.isRunning).toBe(false);
    expect(result.current.queueLength).toBe(0);
  });

  it("reports an unmatched intent and resets isRunning for retry", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Choose",
      options: [{ id: "opt_1", label: "Option 1" }],
    };

    const { result } = renderHook(() => usePipelineRunner(choice, onSubmit));

    const intents: SemanticIntent[] = [
      {
        predicate: (opt) => opt.id === "nonexistent",
      },
    ];

    await act(async () => {
      result.current.executePipeline(intents);
    });

    expect(onSubmit).not.toHaveBeenCalled();
    expect(result.current.isRunning).toBe(false);
    expect(result.current.queueLength).toBe(0);
    expect(result.current.lastError).toMatch(/no longer available/i);
  });

  it("does not dispatch the next intent before a newer choice is available", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Choose",
      options: [
        { id: "opt1", label: "First" },
        { id: "opt2", label: "Second" },
      ],
    };
    const { result, rerender } = renderHook(({ pending }) => usePipelineRunner(pending, onSubmit), {
      initialProps: { pending: choice },
    });
    await act(async () => {
      result.current.executePipeline([
        { predicate: (o) => o.id === "opt1" },
        { predicate: (o) => o.id === "opt2" },
      ]);
    });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    await act(async () => {
      rerender({ pending: choice });
    });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    await act(async () => {
      rerender({ pending: { ...choice, nonce: "n2" } });
    });
    expect(onSubmit).toHaveBeenNthCalledWith(2, "opt2");
  });

  it("keeps the remaining intent across a gap with no pending choice", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Pay",
      context: { subtype: "pay_resources" },
      options: [{ id: "trade_good", label: "Spend" }],
    };
    const { result, rerender } = renderHook(({ pending }) => usePipelineRunner(pending, onSubmit), {
      initialProps: { pending: choice as PendingChoiceDto | null },
    });
    await act(async () => {
      result.current.executePipeline([
        { predicate: (option) => option.id === "trade_good" },
        { predicate: (option) => option.id === "trade_good" },
      ]);
    });
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("trade_good");
    await act(async () => rerender({ pending: null }));
    expect(result.current.isRunning).toBe(true);
    expect(result.current.queueLength).toBe(1);
    expect(onSubmit).toHaveBeenCalledTimes(1);
    await act(async () => rerender({ pending: { ...choice, nonce: "n2" } }));
    expect(onSubmit).toHaveBeenCalledTimes(2);
    expect(result.current.isRunning).toBe(false);
  });

  it("reports an interrupted multi-step queue without sending an unrelated option", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Choose",
      options: [{ id: "opt1", label: "First" }],
    };
    const { result, rerender } = renderHook(({ pending }) => usePipelineRunner(pending, onSubmit), {
      initialProps: { pending: choice },
    });
    await act(async () => {
      result.current.executePipeline([
        { predicate: (o) => o.id === "opt1" },
        { predicate: (o) => o.id === "opt2" },
      ]);
    });
    await act(async () => {
      rerender({
        pending: { ...choice, nonce: "n2", options: [{ id: "unexpected", label: "Other" }] },
      });
    });
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("opt1");
    expect(result.current.isRunning).toBe(false);
    expect(result.current.queueLength).toBe(0);
    expect(result.current.lastError).toMatch(/no longer available/i);
  });

  it("stops when a new workflow reuses the same option ID", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Pay",
      context: { subtype: "pay_resources" },
      options: [{ id: "trade_good", label: "Spend" }],
    };
    const { result, rerender } = renderHook(({ pending }) => usePipelineRunner(pending, onSubmit), {
      initialProps: { pending: choice },
    });
    await act(async () =>
      result.current.executePipeline([
        { predicate: (o) => o.id === "trade_good" },
        { predicate: (o) => o.id === "trade_good" },
      ]),
    );
    await act(async () =>
      rerender({
        pending: { ...choice, nonce: "n2", context: { subtype: "propose_transaction" } },
      }),
    );
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("trade_good");
    expect(result.current.lastError).toMatch(/different decision/);
  });

  it("resets isRunning and captures error if submitChoice rejects", async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error("Network failure"));

    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Choose",
      options: [{ id: "opt_1", label: "Option 1" }],
    };

    const { result } = renderHook(() => usePipelineRunner(choice, onSubmit));

    const intents: SemanticIntent[] = [
      {
        predicate: (opt) => opt.id === "opt_1",
      },
    ];

    await act(async () => {
      result.current.executePipeline(intents);
    });

    expect(onSubmit).toHaveBeenCalledWith("opt_1");
    expect(result.current.isRunning).toBe(false);
    expect(result.current.lastError).toBe("Network failure");
  });

  it("cancelPipeline clears active queue and resets isRunning", async () => {
    let resolveSubmission!: () => void;
    const onSubmit = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveSubmission = resolve;
        }),
    );

    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "n1",
      prompt: "Choose",
      options: [{ id: "opt1", label: "Option 1" }],
    };

    const { result } = renderHook(() => usePipelineRunner(choice, onSubmit));

    act(() => {
      result.current.executePipeline([{ predicate: (o) => o.id === "opt1" }]);
    });

    expect(result.current.isRunning).toBe(true);

    act(() => {
      result.current.cancelPipeline();
    });

    expect(result.current.isRunning).toBe(false);
    expect(result.current.queueLength).toBe(0);

    // Cleanup pending promise
    await act(async () => {
      resolveSubmission();
    });
  });
});
