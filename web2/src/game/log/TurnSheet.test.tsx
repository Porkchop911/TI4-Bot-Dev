import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useFreshQuestion } from "./TurnSheet";

describe("useFreshQuestion", () => {
  const run = (history: boolean, asked: boolean, question: string) => ({
    history,
    asked,
    question,
  });

  it("is not new for the question that was open when the history mode began", () => {
    const hook = renderHook(
      (props) => useFreshQuestion(props.history, props.asked, props.question),
      {
        initialProps: run(false, true, "1:"),
      },
    );
    expect(hook.result.current).toBe(false);
    hook.rerender(run(true, true, "1:"));
    expect(hook.result.current).toBe(false);
  });

  it("marks a question that comes in the history mode, until the mode ends", () => {
    const hook = renderHook(
      (props) => useFreshQuestion(props.history, props.asked, props.question),
      {
        initialProps: run(true, true, "1:"),
      },
    );
    hook.rerender(run(true, true, "2:"));
    expect(hook.result.current).toBe(true);
    hook.rerender(run(false, true, "2:"));
    expect(hook.result.current).toBe(false);
    // The next history mode starts with that question open: it is not new.
    hook.rerender(run(true, true, "2:"));
    expect(hook.result.current).toBe(false);
  });

  it("marks a decision where there was none when the mode began", () => {
    const hook = renderHook(
      (props) => useFreshQuestion(props.history, props.asked, props.question),
      {
        initialProps: run(true, false, "1:"),
      },
    );
    expect(hook.result.current).toBe(false);
    hook.rerender(run(true, true, "2:"));
    expect(hook.result.current).toBe(true);
  });
});
