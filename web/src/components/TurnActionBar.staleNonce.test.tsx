import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

// Nightly 2026-10-14 #8 (o1-43-2146 and 10 other offline runs): a click on the turn bar of a
// question that was already answered sent `strategic` with the next question's nonce. The bar
// now says which question it showed, so the transport can drop a stale click.
describe("turn bar submissions name their question", () => {
  it("passes the nonce of the turn menu it was drawn for", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "p1",
      nonce: "menu-45",
      prompt: "action phase",
      context: { subtype: "prompt:action phase" } as PendingChoiceDto["context"],
      options: [
        { id: "strategic", kind: "action", label: "take your strategic action", payload: {} },
        { id: "tactical", kind: "action", label: "take a tactical action", payload: {} },
      ],
      details: {
        kind: "turn_menu",
        closing: false,
        strategy_cards: [{ card: "pok3politics", used: false, option: "strategic" }],
      },
    } as PendingChoiceDto;
    render(
      <ChoiceRendererDispatcher
        choice={choice}
        viewerSeat="p1"
        players={{ p1: { id: "p1", faction: "nomad" } } as never}
        onSubmit={onSubmit}
        isMinimized={false}
        onMinimizedChange={() => undefined}
      />,
    );
    fireEvent.click(screen.getByTestId("turn-bar-strategic-pok3politics"));
    expect(onSubmit).toHaveBeenCalledWith("strategic", "menu-45");
  });
});
