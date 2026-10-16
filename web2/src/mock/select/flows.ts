// Actions that are not tactical: the same panel frame with other steps.
import type {
  BlockView,
  FlowActionView,
  FooterView,
  HelpView,
  InterruptView,
  MenuRowView,
  PaymentView,
  PillView,
  RowView,
  SeatRowView,
} from "../../model";
import { plural } from "../../model";
import { ACTION_CARDS, CARDS, DECISIONS, PAY, SEATS } from "../data";
import { E } from "../loose";
import { STRATEGY_TEXT, strategyEditor } from "../strategy";
import { heldCards, sysLabel, type State, type World } from "../world";
import { END_TURN, button, closingBlocks, reactionParts, tradeRow, withKeys } from "./shared";

type Parts = {
  title: [string, string];
  heading: string;
  pill: PillView | null;
  trail?: FlowActionView["trail"];
  help: HelpView[];
  blocks: BlockView[];
  footer: FooterView;
  interrupt?: InterruptView;
};

function flowParts(world: World, state: State): Parts {
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
  if (state.kind === "waiting") {
    const name = SEATS[flow.owner].name;
    return {
      title: [
        flow.passed ? "You passed" : "Turn ended",
        `${name} (${SEATS[flow.owner].faction}) is next`,
      ],
      heading: `${name}’s turn`,
      pill: { tone: "quiet", label: `Waiting for ${name}` },
      help: [],
      blocks: [
        {
          kind: "note",
          tone: "quiet",
          strong: flow.passed ? "You passed." : "Your turn ended.",
          text: flow.passed
            ? "You take no more actions in this round."
            : "You can prepare your next tactical action in Draft.",
        },
      ],
      footer: foot(`Waiting for ${name}.`, flow.passed ? [] : sim),
    };
  }
  if (state.kind === "picker") {
    if (flow.menu === "actionCards") {
      return {
        title: ["Your turn", "Choose an action card"],
        heading: "Play an action card",
        pill: { tone: "live", label: "Your decision" },
        help: [],
        blocks: [
          {
            kind: "menu",
            rows: world.table.actionCards.map((card, index) => ({
              key: `${index + 1}`,
              title: card,
              text: ACTION_CARDS[card] ?? DECISIONS[card]?.text,
              disabled: false,
              intent: { type: "pickAction", action: "component", card },
            })),
          },
        ],
        footer: foot("Choose 1 action card.", [
          button({ type: "backToPicker" }, "Back to actions"),
        ]),
      };
    }
    // Fixed groups in a fixed order. A row that cannot be used stays in its place and says why.
    const prepared = world.workspaces.draft;
    const held = heldCards(world);
    const unused = held.filter((number) => !world.usedCards.includes(number));
    const cards = world.table.actionCards;
    const rows: MenuRowView[] = [
      {
        group: "Tactical",
        key: "t",
        title: prepared.done[0] ? "Open your draft" : "Activate a system",
        state: prepared.done[0]
          ? sysLabel(prepared.data.activation.system)
          : `${SEATS.sol.tokens[0]} tactic tokens`,
        hint: prepared.done[0]
          ? "The tactical action that you prepared. It stays private until you apply it."
          : "Then move ships, fight, invade, and produce there. You choose the system on the board.",
        disabled: false,
        intent: { type: "pickAction", action: "tactical" },
      },
      // One row for each strategy card. The key is the number of the card.
      ...held.map((number, index): MenuRowView => {
        const used = !unused.includes(number);
        return {
          group: index ? undefined : "Strategic",
          key: `${number}`,
          title: `${number} ${CARDS[number - 1]}`,
          state: used ? "Used in this round" : undefined,
          hint: STRATEGY_TEXT[number].primary,
          disabled: used,
          intent: { type: "pickAction", action: "strategic", card: `${number}` },
        };
      }),
      // One row for the action cards and one for the other components, whatever the player holds.
      // The second step asks which one. So the list is the same for every turn of a game.
      {
        group: "Component",
        key: "a",
        title: "Play an action card",
        state: cards.length ? `${cards.length} playable` : "None is playable",
        disabled: !cards.length,
        intent: { type: "pickGroup", group: "actionCards" },
      },
      {
        key: "c",
        title: "Use a technology, leader or planet",
        state: "None is ready",
        disabled: true,
        intent: { type: "pickAction", action: "component" },
      },
      tradeRow,
      {
        group: "Pass",
        key: "p",
        title: "Take no more actions in this round",
        state: unused.length
          ? `Use ${unused.map((number) => `${number} ${CARDS[number - 1]}`).join(" and ")} first`
          : undefined,
        hint: "You must use your strategy cards before you can pass.",
        disabled: unused.length > 0,
        intent: { type: "pass" },
      },
    ];
    return {
      title: ["Your turn", "Choose one action"],
      heading: "Choose an action",
      pill: { tone: "live", label: "Your decision" },
      blocks: [{ kind: "menu", rows }],
      help: [
        {
          title: "Not your turn?",
          text: "This panel shows the acting player's action, and you can draft your next tactical action in Draft.",
        },
        {
          title: "Nothing is sent by a choice",
          text: "A choice opens the action as a draft. The main button of the footer sends it. Esc goes back.",
        },
      ],
      footer: foot("Your turn. Choose one action."),
    };
  }
  if (state.kind === "decision") {
    const decision = DECISIONS[flow.decision];
    const done = flow.stage === "done";
    const staged: boolean = E.decisionStaged(state);
    const ready: boolean = !done && E.decisionReady(state);
    const me = SEATS.sol;
    const source: BlockView = {
      kind: "source",
      name: flow.decision,
      type: decision.type,
      text: decision.text,
    };
    const selected: RowView["control"] = {
      kind: "badge",
      pill: { tone: "draft", label: "Selected" },
    };
    // One of the shapes. The panel shows the choice; the board or the player table is the picker.
    let shape: BlockView[] = [];
    let main = `Play ${flow.decision}`;
    let note = decision.ask + ".";
    if (decision.shape === "system") {
      shape = [
        {
          kind: "card",
          title: "System",
          aside: "Choose on the map",
          rows: flow.chosen
            ? [
                {
                  icon: "token",
                  title: [sysLabel(flow.chosen)],
                  subtitle: `Tactic pool ${me.tokens[0]} · the token goes to your reinforcements`,
                  link: [`sys:${flow.chosen}`],
                  control: selected,
                },
              ]
            : [{ icon: "token", title: ["No system chosen"] }],
        },
      ];
    } else if (decision.shape === "seat") {
      shape = [
        {
          kind: "card",
          title: "Player",
          aside: "Choose in the player table",
          rows: flow.chosen
            ? [
                {
                  icon: "card",
                  title: [{ seat: flow.chosen }, ` ${SEATS[flow.chosen].name}`],
                  subtitle: `${SEATS[flow.chosen].faction} · ${plural(SEATS[flow.chosen].ac, "action card")} · you take 1 at random`,
                  control: selected,
                },
              ]
            : [{ icon: "card", title: ["No player chosen"] }],
        },
      ];
    } else if (decision.shape === "list") {
      main = "Confirm";
      const answers: [string, string, string][] = [
        ["replenish", "Replenish commodities", `Commodities ${me.comm[0]} → ${me.comm[1]}`],
        [
          "convert",
          "Convert commodities to trade goods",
          `Trade goods ${me.tg} → ${me.tg + me.comm[0]} · Commodities ${me.comm[0]} → 0`,
        ],
      ];
      shape = [
        {
          kind: "menu",
          rows: answers.map(([id, title, effect], index) => ({
            key: `${index + 1}`,
            title,
            state: effect,
            selected: flow.chosen === id,
            disabled: false,
            intent: { type: "chooseOption", option: id },
          })),
        },
      ];
    } else {
      const units = E.unitsOver(state);
      main = units.removed ? `Remove ${plural(units.removed, "ship")}` : "Remove ships";
      note =
        units.left > units.limit
          ? `Remove ${plural(units.left - units.limit, "ship")} more.`
          : "This commits to the live game.";
      shape = [
        {
          kind: "gauges",
          gauges: [
            {
              label: `Fleet supply in ${sysLabel(units.system)}`,
              used: units.left,
              total: units.limit,
            },
          ],
        },
        {
          kind: "card",
          title: `Your ships in ${sysLabel(units.system)}`,
          aside: `${units.removed} to remove`,
          bad: units.left > units.limit,
          rows: units.lines.map((line: any) => ({
            title: [E.unitName(line.type, line.n)],
            subtitle: `${line.n} here${flow.counts[line.id] ? ` → ${line.n - flow.counts[line.id]}` : ""}`,
            link: [`sys:${units.system}`],
            control: {
              kind: "counter",
              counter: {
                id: line.id,
                value: flow.counts[line.id] || 0,
                max: line.n,
                label: `removed ${E.unitName(line.type).toLowerCase()}`,
              },
            },
          })),
        },
      ];
    }
    return {
      title: [
        flow.card ? "Component action" : "Decision",
        `${flow.decision} · ${me.name} (${me.faction})`,
      ],
      // The card is in the source block; the heading says what the player must do.
      heading: decision.ask,
      pill: done
        ? { tone: "done", label: "Resolved" }
        : staged
          ? { tone: "draft", label: "Not sent" }
          : { tone: "live", label: "Your decision" },
      help: [],
      blocks: done
        ? [source, { kind: "note", tone: "success", strong: "Resolved.", text: flow.result }]
        : [source, ...shape],
      footer: foot(
        done
          ? flow.card
            ? "Component action complete."
            : "Decision resolved."
          : ready && decision.shape !== "units"
            ? "This commits to the live game."
            : note,
        done
          ? []
          : [
              ...(staged ? [button({ type: "clearChoice" }, "Clear choice")] : []),
              button({ type: "flow", action: "play" }, main, "primary", !ready, true),
            ],
        decision.shape === "units" && !ready,
      ),
    };
  }
  if (state.kind === "component" && flow.stage === "reaction") {
    // Another player's action card, and the viewer holds a card that reacts to it. The panel
    // keeps that player's action; the reaction window is on top of it.
    const owner = SEATS[flow.owner];
    const reaction = reactionParts(
      `After ${owner.name} plays ${flow.card}`,
      [],
      flow.reaction.cards,
      flow.reaction.pick,
    );
    return {
      title: ["Component action", `${flow.card} · ${owner.name} (${owner.faction})`],
      heading: flow.card,
      pill: flow.reaction.pick
        ? { tone: "draft", label: "Not sent" }
        : { tone: "live", label: "Your decision · Reaction" },
      help: [],
      blocks: [
        { kind: "source", name: flow.card, type: "Action card", text: ACTION_CARDS[flow.card] },
        {
          kind: "note",
          tone: "quiet",
          strong: `${owner.name} plays ${flow.card}.`,
          text: "It resolves when every player has passed.",
        },
      ],
      footer: foot(reaction.note, reaction.actions),
      interrupt: reaction.interrupt,
    };
  }
  if (state.kind === "component") {
    const done = flow.stage === "done";
    const own = flow.owner === "sol";
    // A card with a target: the player chooses the planet on the board. The panel shows the choice only.
    const targeted = flow.stage === "target" || !!flow.target;
    const rows: RowView[] = PAY.filter((source) => source.id === flow.target).map((source) => ({
      icon: "planet",
      title: [`${source.label.replace("Exhaust ", "")} · #${source.system}`],
      subtitle: `${plural(source.res, "resource")} → +${plural(source.res, "trade good")}`,
      link: [`pl:${source.id}`, `sys:${source.system}`],
      control: { kind: "badge", pill: { tone: "live", label: "Selected" } },
    }));
    if (!rows.length) {
      rows.push({ icon: "planet", title: ["No planet chosen"] });
    }
    const ready = !targeted || !!flow.target;
    return {
      title: [
        "Component action",
        `${flow.card} · ${SEATS[flow.owner].name} (${SEATS[flow.owner].faction})`,
      ],
      heading: flow.card,
      pill: {
        tone: done ? "done" : "live",
        label: done
          ? "Resolved"
          : targeted
            ? "Your decision · Choose a planet"
            : "Your decision · Play the card",
      },
      trail: [
        ...(targeted ? [{ label: "Target", status: done ? "complete" : "active" } as const] : []),
        { label: "Result", status: done ? "complete" : "todo" },
      ],
      help: [{ title: `Action card · ${flow.card}`, text: ACTION_CARDS[flow.card] ?? "" }],
      blocks: [
        done
          ? { kind: "note", tone: "success", strong: "Resolved.", text: flow.result }
          : targeted
            ? { kind: "card", title: "Planet", aside: "Choose on the map", rows }
            : {
                kind: "card",
                title: "Action card",
                rows: [
                  {
                    icon: "card",
                    title: [flow.card],
                    subtitle: ACTION_CARDS[flow.card],
                    control: { kind: "badge", pill: { tone: "live", label: "Ready" } },
                  },
                ],
              },
      ],
      footer: foot(
        done
          ? "Component action complete."
          : own && ready
            ? "This commits to the live game."
            : "Choose 1 planet you control.",
        done
          ? []
          : [
              button(
                { type: "flow", action: "play" },
                `Play ${flow.card}`,
                "primary",
                !ready,
                true,
              ),
            ],
      ),
    };
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
    const editor = strategyEditor(state, world.table.seats);
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
  const reaction = state.kind === "component" && flow.stage === "reaction";
  const seat: string = state.kind === "waiting" || reaction ? flow.owner : "sol";
  const name: string = SEATS[seat].name;
  const who = `${name} (${SEATS[seat].faction})`;
  const card: string | undefined = state.kind === "decision" ? flow.decision : flow.card;
  if (reaction || (card && flow.stage === "done")) {
    return {
      title: [
        state.kind === "component" || flow.card ? "Component action" : "Decision",
        `${card} · ${who}`,
      ],
      heading: card!,
      pill: reaction
        ? { tone: "quiet", label: "Waiting for reactions" }
        : { tone: "done", label: "Resolved" },
      help: [],
      blocks: [
        {
          kind: "source",
          name: card!,
          type: state.kind === "decision" ? DECISIONS[card!].type : "Action card",
          text: (state.kind === "decision" ? DECISIONS[card!].text : ACTION_CARDS[card!]) ?? "",
        },
        reaction
          ? {
              kind: "note",
              tone: "quiet",
              strong: `${name} plays ${card}.`,
              text: "It resolves when every player has passed.",
            }
          : { kind: "note", tone: "success", strong: "Resolved.", text: flow.result },
      ],
      footer: foot(reaction ? "Waiting for the other players." : "Action complete."),
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
  const editor = strategyEditor(state, []);
  return {
    status: { tone: "draft", sign: "✎", label: "Draft ready" },
    text: `${editor.pass ? "Pass" : `Follow: ${editor.result.replace("Followed · ", "")}`} · not sent, resolves after ${before}`,
  };
}

export function selectFlow(world: World, state: State): FlowActionView {
  const parts = flowParts(world, state);
  const done = !E.flowNeed(state);
  const staged: boolean = E.flowStaged(state);
  const closing = world.mode === "live" ? closingBlocks(state) : [];
  // Before the first commit the player can drop the action and choose another one.
  if (staged) {
    parts.footer.actions.unshift(button({ type: "backToPicker" }, "Back to actions"));
  }
  if (closing.length) {
    parts.footer.actions.push(END_TURN);
  }
  withKeys(parts.footer);
  return {
    kind: "flow",
    title: parts.title[0],
    subtitle: parts.title[1],
    badge:
      state.kind === "picker" || state.kind === "waiting"
        ? null
        : staged
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
    interrupt: world.mode === "history" ? null : (parts.interrupt ?? null),
    closing,
    footer: parts.footer,
  };
}
