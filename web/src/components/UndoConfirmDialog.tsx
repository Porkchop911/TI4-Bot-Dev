import React from "react";
import { Dialog } from "../primitives/index.ts";

export interface UndoConfirmDialogProps {
  /** How many decisions are rolled back; 1 for "undo the latest action". */
  steps: number;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Styled replacement for window.confirm: undo is shared history, so say who it affects. */
export const UndoConfirmDialog: React.FC<UndoConfirmDialogProps> = ({
  steps,
  onConfirm,
  onCancel,
}) => (
  <Dialog.Root open onOpenChange={(open) => !open && onCancel()}>
    <Dialog.Content data-testid="undo-confirm-dialog" className="choice-workflow-dialog">
      <div className="panel choice-workflow-modal undo-confirm">
        <Dialog.Title as="h2" className="choice-workflow-title">
          Undo for everyone?
        </Dialog.Title>
        <Dialog.Description>
          {steps > 1
            ? `This rolls the game back by ${steps} decisions for every player in this game.`
            : "This undoes the latest action and its follow-up decisions for every player in this game."}
        </Dialog.Description>
        <div className="undo-confirm__actions">
          <button
            type="button"
            className="button button--secondary"
            data-testid="undo-confirm-cancel"
            onClick={onCancel}
          >
            Keep playing
          </button>
          <button
            type="button"
            className="button button--primary"
            data-testid="undo-confirm-accept"
            onClick={onConfirm}
          >
            Undo for everyone
          </button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>
);
