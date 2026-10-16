// Actions that are not tactical: the same panel frame with other steps.
import type {
  BlockView,
  FlowActionView,
  FooterView,
  HelpView,
  PaymentView,
  PillView,
  SeatRowView,
} from "../../model";
import { ACTION_CARDS, SEATS } from "../data";
import { E } from "../loose";
import { STRATEGY_TEXT, strategyEditor } from "../strategy";
import type { State, World } from "../world";
import { END_TURN, button, closingBlocks, withKeys } from "./shared";

type Parts = {
  title: [string, string];
  heading: string;
  pill: PillView | null;
  trail?: FlowActionView["trail"];
  help: HelpView[];
  blocks: BlockView[];
  footer: FooterView;
};

function flowParts(state: State): Parts {
  const flow = state.flow;
  const wait = E.flowNeed(state);
  const foot = (
    note: string,
    actions: FooterView["actions"] = [],
    error = false,
    payment?: PaymentView,
  ): FooterView => ({ note, actions, error, payment });
  const sim =
    wait && !wait.mine
      ? [button({ type: "simulate" }, `Simulate ${SEATS[wait.seat].name} (demo)`, "demo")]
      : [];
  const viewer: string = E.env.viewer;
  if (!E.own() && state.kind !== "strategic") {
    return publicParts(state, foot);
  }
  // A resolved card of another player: its result.
  if (state.kind === "component") {
    return publicParts(state, foot);
  }
  const mine: string | null = E.flowOwnRow(state);
  const myTurn = E.own() && flow.stage === "secondary" && flow.order[flow.turn] === "sol";
  const text = STRATEGY_TEXT[flow.number ?? 1];
  const base = {
    title: [
      "Strategic action",
      `${flow.card} · ${SEATS[flow.owner].name} (${SEATS[flow.owner].faction})`,
    ] as [string, string],
    heading: flow.card as string,
    help: [
      { title: `${flow.card} · Primary`, text: text.primary },
      { title: `${flow.card} · Secondary`, text: text.secondary },
    ] as HelpView[],
  };
  // One list: every seat with what it did. The viewer's row opens into `open`.
  const list = (open: BlockView[] | null): BlockView[] => [
    {
      kind: "seats",
      rows: ["primary", ...flow.order].map((row: string, index: number) => {
        const seat: string = row === "primary" ? flow.owner : row;
        return {
          order: index ? `${index}` : "P",
          seat,
          name: seat === viewer ? "You" : SEATS[seat].name,
          you: seat === viewer,
          ...seatLine(state, row),
          open: open && row === mine ? open : undefined,
        };
      }),
    },
  ];
  const waiting =
    flow.stage === "done"
      ? "Strategic action complete."
      : `Waiting for ${SEATS[wait.seat].name} · ${flow.card} ${flow.stage === "primary" ? "primary" : "secondary"}.`;
  // The viewer's own row with an open choice: the editor.
  if (E.flowEditing(state)) {
    const editor = strategyEditor(state);
    const error = editor.problem;
    const clear = editor.staged ? [button({ type: "clearChoice" }, "Clear choice")] : [];
    const payment = editor.task?.payment;
    if (mine === "primary") {
      return {
        ...base,
        pill: { tone: "live", label: "Your decision" },
        blocks: list(editor.blocks),
        footer: foot(
          error || "This commits to the live game.",
          [
            ...clear,
            button(
              { type: "flow", action: "resolve" },
              editor.main ?? "Resolve primary",
              "primary",
              !!error,
              true,
            ),
          ],
          !!error,
          payment,
        ),
      };
    }
    return {
      ...base,
      pill: myTurn
        ? { tone: "live", label: "Your decision" }
        : { tone: "draft", label: "Private draft" },
      blocks: list(editor.blocks),
      footer: foot(
        error ||
          (myTurn ? "This commits to the live game." : "Private until your seat is reached."),
        [
          ...(myTurn ? [] : sim),
          ...clear,
          myTurn
            ? button(
                { type: "flow", action: "resolve" },
                editor.pass ? "Resolve secondary · pass" : "Resolve secondary",
                "primary",
                !!error,
                true,
              )
            : button(
                { type: "flow", action: "ready" },
                editor.pass ? "Mark draft ready · pass" : "Mark draft ready",
                "primary",
                !!error,
              ),
        ],
        !!error,
        payment,
      ),
    };
  }
  // The viewer's secondary, drafted and ready: it waits for the seat.
  if (E.flowOpen(state) && flow.mine.ready) {
    return {
      ...base,
      pill: { tone: "draft", label: "Draft ready" },
      blocks: list(null),
      footer: foot("Your draft waits for your seat. You can still change it.", [
        ...sim,
        button({ type: "flow", action: "change" }, "Change draft"),
      ]),
    };
  }
  return {
    ...base,
    pill:
      flow.stage === "done"
        ? { tone: "done", label: "Resolved" }
        : { tone: "quiet", label: `Waiting for ${SEATS[wait.seat].name}` },
    blocks: list(null),
    footer: foot(waiting, sim),
  };
}

/**
 * What another viewer sees of an action that is not a strategy card: the result when it is
 * resolved, and who the game waits for before. A staged choice is private.
 */
function publicParts(
  state: State,
  foot: (note: string, actions?: FooterView["actions"]) => FooterView,
): Parts {
  const flow = state.flow;
  const seat: string = flow.owner ?? "sol";
  const name: string = SEATS[seat].name;
  const who = `${name} (${SEATS[seat].faction})`;
  const card: string | undefined = flow.card;
  if (card && flow.stage === "done") {
    return {
      title: ["Component action", `${card} · ${who}`],
      heading: card!,
      pill: { tone: "done", label: "Resolved" },
      help: [],
      blocks: [
        {
          kind: "source",
          name: card!,
          type: "Action card",
          text: ACTION_CARDS[card!] ?? "",
        },
        { kind: "note", tone: "success", strong: "Resolved.", text: flow.result },
      ],
      footer: foot("Action complete."),
    };
  }
  return {
    title: [`${name}’s turn`, who],
    heading: `${name}’s turn`,
    pill: { tone: "quiet", label: `Waiting for ${name}` },
    help: [],
    blocks: [],
    footer: foot(`Waiting for ${name}.`),
  };
}

type SeatLine = Pick<SeatRowView, "status" | "text">;

/** The state of one row of the seat order, and what that seat did. The sign carries the state. */
function seatLine(state: State, row: string): SeatLine {
  const flow = state.flow;
  if (row === "primary") {
    return flow.stage === "primary" && !flow.primaryResult
      ? { status: { tone: "live", sign: "▶", label: "Deciding" } }
      : { status: { tone: "done", sign: "✓", label: "Resolved" }, text: flow.primaryResult };
  }
  const index: number = flow.order.indexOf(row);
  const prior: string = index ? flow.order[index - 1] : flow.owner;
  const before = prior === E.env.viewer ? "you" : SEATS[prior].name;
  const mine = row === "sol" && E.own();
  const result: string | undefined = flow.results[row];
  if (result) {
    return result.startsWith("Followed")
      ? {
          status: { tone: "done", sign: "✓", label: "Followed" },
          text: result.replace("Followed · ", ""),
        }
      : { status: { tone: "quiet", sign: "–", label: "Passed" } };
  }
  if (flow.stage === "secondary" && flow.turn === index) {
    return {
      status: { tone: "live", sign: "▶", label: mine ? "Your decision" : "Deciding" },
    };
  }
  if (!mine) {
    return { status: { tone: "quiet", sign: "·", label: `Waits for ${before}` } };
  }
  if (!flow.mine.ready) {
    return {
      status: { tone: "draft", sign: "✎", label: "Private draft" },
      text: `Your seat is after ${before}.`,
    };
  }
  // A ready draft: what it will do, visible to the viewer only.
  const editor = strategyEditor(state);
  return {
    status: { tone: "draft", sign: "✎", label: "Draft ready" },
    text: `${editor.pass ? "Pass" : `Follow: ${editor.result.replace("Followed · ", "")}`} · not sent, resolves after ${before}`,
  };
}

export function selectFlow(world: World, state: State): FlowActionView {
  const parts = flowParts(state);
  const done = !E.flowNeed(state);
  const staged: boolean = E.flowStaged(state);
  const closing = world.mode === "live" ? closingBlocks(state) : [];
  if (closing.length) {
    parts.footer.actions.push(END_TURN);
  }
  withKeys(parts.footer);
  return {
    kind: "flow",
    title: parts.title[0],
    subtitle: parts.title[1],
    badge: staged
      ? { tone: "draft", label: "Not sent" }
      : { tone: done ? "done" : "live", label: done ? "Complete" : "In progress" },
    past: world.mode === "history" ? { label: state.past.label } : null,
    tabs: [],
    current: null,
    selected: state.selected,
    contentKey: `${state.kind}:${state.selected}`,
    heading: parts.heading,
    pill: parts.pill,
    trail: parts.trail ?? [],
    help: parts.help,
    blocks: parts.blocks,
    interrupt: null,
    closing,
    footer: parts.footer,
  };
}
