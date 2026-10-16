const TYPING = "input, select, textarea, [contenteditable]";
const PRESSABLE = "button, a, summary, [role=button], [role=tab], [role=menuitem]";
export const OVERLAY = "dialog[open], [role=menu], [role=note]";

/**
 * False when the key belongs to something else: a text field, an open dialog, menu or rule text,
 * or (for Enter) a control that the player reached with the keyboard. A control that only has
 * focus because of a click does not keep Enter: the player clicks on the board or a counter,
 * then confirms with Enter.
 */
export function keyIsFree(event: KeyboardEvent, focusByKeyboard: boolean): boolean {
  if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) {
    return false;
  }
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest(TYPING) || document.querySelector(OVERLAY)) {
    return false;
  }
  // `:focus-visible` cannot tell: the browser sets it for the Enter press itself.
  return !(event.key === "Enter" && focusByKeyboard && target?.closest(PRESSABLE));
}
