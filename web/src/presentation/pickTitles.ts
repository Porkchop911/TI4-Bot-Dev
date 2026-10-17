import type { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describeLawOption } from "./lawPick.ts";
import { describeTechnologyPickOption } from "./technologyPick.ts";
import { describeUnitPickOption } from "./unitPick.ts";

/**
 * The name an option of an action card's pick goes by, in place of the engine's label (which can
 * hold a raw id or a seat id): the technology, law or unit it stands for. `null` for any other option.
 */
export function pickOptionTitle(
  choice: PendingChoiceDto,
  option: ChoiceOptionDto,
  board?: BoardView | null,
): string | null {
  return (
    describeTechnologyPickOption(choice, option)?.name ??
    describeLawOption(choice, option)?.name ??
    describeUnitPickOption(choice, option, board)?.title ??
    null
  );
}
