import { expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { UndoConfirmDialog } from "./UndoConfirmDialog.tsx";

it("asks once in a styled dialog and names everyone as affected", () => {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(<UndoConfirmDialog steps={3} onConfirm={onConfirm} onCancel={onCancel} />);
  const dialog = screen.getByRole("dialog", { name: "Undo for everyone?" });
  expect(dialog).toHaveTextContent("3 decisions for every player");
  fireEvent.click(screen.getByTestId("undo-confirm-cancel"));
  expect(onCancel).toHaveBeenCalledOnce();
  expect(onConfirm).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Undo for everyone" }));
  expect(onConfirm).toHaveBeenCalledOnce();
});

it("cancels on Escape and describes a single-step undo", () => {
  const onCancel = vi.fn();
  render(<UndoConfirmDialog steps={1} onConfirm={vi.fn()} onCancel={onCancel} />);
  expect(screen.getByTestId("undo-confirm-dialog")).toHaveTextContent("latest action");
  fireEvent.keyDown(screen.getByTestId("undo-confirm-dialog"), { key: "Escape" });
  expect(onCancel).toHaveBeenCalled();
});
