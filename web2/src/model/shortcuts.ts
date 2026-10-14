import type { ActionButtonView, ActionView, BlockView } from "./action";
import type { Intent } from "./intents";

type Keyed = Pick<ActionButtonView, "key" | "disabled" | "intent">;

function collect(blocks: BlockView[], into: Keyed[]) {
  for (const block of blocks) {
    if (block.kind === "card") {
      for (const row of block.rows) {
        if (row.control?.kind === "button") {
          into.push(row.control.button);
        }
      }
    }
    if (block.kind === "menu") {
      into.push(...block.rows);
    }
    if (block.kind === "draft") {
      collect(block.blocks, into);
    }
    if (block.kind === "seats") {
      for (const row of block.rows) {
        collect(row.open ?? [], into);
      }
    }
  }
}

/** The shortcut keys of the open action, in lower case, with what they send. Disabled buttons have none. */
export function shortcuts(view: ActionView): Map<string, Intent> {
  const buttons: Keyed[] = [];
  // A reaction window is on top: its keys are the only keys of the content. The footer stays.
  if (view.interrupt) {
    collect(view.interrupt.blocks, buttons);
  } else {
    if (view.kind === "flow") {
      collect(view.blocks, buttons);
    }
    collect(view.closing, buttons);
  }
  buttons.push(...(view.kind === "tactical" ? view.task.footer : view.footer).actions);
  const keys = new Map<string, Intent>();
  for (const button of buttons) {
    if (button.key && !button.disabled && !keys.has(button.key.toLowerCase())) {
      keys.set(button.key.toLowerCase(), button.intent);
    }
  }
  return keys;
}
