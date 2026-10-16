// The screens of several questions of the live game: the game asks one question at a time, and
// an editor stages all of them on one screen, then answers them at once. Each editor is chosen by
// the kind and the details of the question, never by the card that asks it (a card of another
// set with the same questions gets the same editor). An editor takes what the player does on the
// map, in the player table and in the panel, and says what its main button sends.
import type { Catalog, Choice, SessionUpdate } from "../wire";
import type { Editor, EditorSlot, EditorView } from "./editor";
import { agendaDeckEditor } from "./agendaDeck";
import { purchaseEditor } from "./purchase";
import { readyEditor } from "./ready";
import { recallEditor } from "./recall";
import { replenishEditor } from "./replenish";
import { scoringEditor } from "./scoring";
import { structuresEditor } from "./structures";

/** The editor of the open question, its screen, and the slot after each change. Null: no change. */
export interface OpenEditor {
  view: EditorView;
  system(system: string): EditorSlot | null;
  planet(planet: string): EditorSlot | null;
  seat(seat: string): EditorSlot | null;
  option(group: string | undefined, option: string): EditorSlot | null;
  part(part: string): EditorSlot | null;
  last(): EditorSlot | null;
  count(key: string, value: number): EditorSlot | null;
  goods(value: number): EditorSlot | null;
  pay(how: "auto" | "reset"): EditorSlot | null;
}

type Opener = (
  update: SessionUpdate,
  choice: Choice,
  slot: EditorSlot | null | undefined,
  printed: Catalog | null,
) => OpenEditor | null;

/** The editor `editor` under the id `id` of its slot. */
function opener<D>(id: string, editor: Editor<D>): Opener {
  return (update, choice, slot, printed) => {
    // A slot holds only what this editor staged for this question: the slot is cleared with each
    // new question, and an editor reads only a slot of its own id.
    const draft = slot?.id === id ? (slot.draft as D) : null;
    const view = editor.view(update, choice, draft, printed);
    const slotOf = (after: D | null | undefined): EditorSlot | null =>
      after === null || after === undefined ? null : { id, draft: after };
    return (
      view && {
        view,
        system: (system) => slotOf(editor.system?.(choice, draft, system)),
        planet: (planet) => slotOf(editor.planet?.(choice, draft, planet)),
        seat: (seat) => slotOf(editor.seat?.(choice, draft, seat)),
        option: (group, option) => slotOf(editor.option?.(choice, draft, group, option)),
        part: (part) => slotOf(editor.part?.(choice, draft, part)),
        last: () => slotOf(editor.last?.(choice, draft)),
        count: (key, value) => slotOf(editor.count?.(choice, draft, key, value)),
        goods: (value) => slotOf(editor.goods?.(choice, draft, value)),
        pay: (how) => slotOf(editor.pay?.(choice, draft, how)),
      }
    );
  };
}

/** The editors, in the order that they are tried. */
const EDITORS: Opener[] = [
  opener("purchase", purchaseEditor),
  opener("ready", readyEditor),
  opener("agendaDeck", agendaDeckEditor),
  opener("structures", structuresEditor),
  opener("replenish", replenishEditor),
  opener("recall", recallEditor),
  opener("scoring", scoringEditor),
];

/**
 * The editor of `choice`, with what is staged in `slot`. Null: no editor has the shape of the
 * question, and it is the list of the game.
 */
export function editorOf(
  update: SessionUpdate,
  choice: Choice,
  slot: EditorSlot | null | undefined,
  printed: Catalog | null = null,
): OpenEditor | null {
  for (const open of EDITORS) {
    const editor = open(update, choice, slot, printed);
    if (editor) {
      return editor;
    }
  }
  return null;
}
