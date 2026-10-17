// The window of a secondary that has no choice but to follow or to pass (`yes` and `no` of a
// question of kind `strategy_secondary`): Trade's replenish, Imperial's secret objective. It is a
// list of two rows, "Follow" with what it does and costs, and "Pass", keys 1 and 2; the main
// button waits for one of them.
import type { BlockView, MenuRowView } from "../../model";
import type { Choice } from "../wire";

/** The question is the window of a secondary with no choice: `yes` and `no`, nothing else. */
export function isFollowWindow(choice: Choice): boolean {
  const ids = new Set(choice.options.map((option) => option.id));
  return (
    choice.details?.kind === "strategy_secondary" &&
    ids.size === 2 &&
    ids.has("yes") &&
    ids.has("no")
  );
}

/**
 * What a secondary costs: "Strategy pool 2 → 1", or "no strategy token" when the question says so.
 * The game does not say who or what makes it free. Null when the question does not say.
 */
export function secondaryCost(details: Record<string, unknown> | undefined): string | null {
  const left = typeof details?.tokens_left === "number" ? details.tokens_left : null;
  return details?.costs_token === false
    ? "no strategy token"
    : details?.costs_token === true && left !== null
      ? `Strategy pool ${left} → ${left - 1}`
      : null;
}

export interface FollowView {
  blocks: BlockView[];
  note: string;
  problem: string | null;
  label: string;
  changed: boolean;
  answers: string[] | null;
}

/**
 * The window as a list: `title` is the row that follows ("Draw 1 secret objective"), `effect` what
 * it changes in game values, `doing` the end of the main button ("draw 1 secret objective").
 */
export function followWindow(
  choice: Choice,
  follow: boolean | null,
  title: string,
  effect: string,
  doing: string,
): FollowView {
  const cost = secondaryCost(choice.details);
  const row = (key: string, option: string, name: string, state?: string): MenuRowView => ({
    key,
    title: name,
    state,
    selected: follow === (option === "yes"),
    disabled: false,
    intent: { type: "chooseOption", option, group: "follow" },
  });
  const problem = follow === null ? `${title}, or pass.` : null;
  return {
    blocks: [
      {
        kind: "menu",
        rows: [
          row("1", "yes", title, [effect, ...(cost ? [cost] : [])].join(" · ")),
          row("2", "no", "Pass"),
        ],
      },
    ],
    note: problem ?? (follow ? effect : "You pass."),
    problem,
    label: follow === null ? "Send" : follow ? `Follow · ${doing}` : "Pass",
    changed: follow !== null,
    answers: follow === null ? null : [follow ? "yes" : "no"],
  };
}

/** The answer of a window: `yes` follows, `no` passes; the same answer again takes it back. */
export function chooseFollow(
  follow: boolean | null,
  group: string | undefined,
  option: string,
): { follow: boolean | null } | null {
  if (group !== "follow" || (option !== "yes" && option !== "no")) {
    return null;
  }
  const chosen = option === "yes";
  return { follow: follow === chosen ? null : chosen };
}
