// A strategic action: one list of seats, the primary and then the secondaries in the order in
// which they are asked. The row of the viewer opens into its choice, or into the private draft
// of its secondary. The choice is that of every decision (`selectAction`): the editor of its
// shape (`editors.ts`), else the list of the game.
import { promptWords } from "./notes";
import type { ActionButtonView, BlockView, FlowActionView, SeatRowView } from "../../model";
import type { Catalog, ExplorationCatalog, SecondaryFacts, SessionUpdate } from "../wire";
import { type LocalState, openChoice, openEditor, selectAction } from "./action";
import { secondaryCost } from "./follow";
import { seatLabel, sentence } from "./names";
import { strategyCardHelp } from "./printed";

/** The seat that the strategic action waits for: the primary, then the first follower to ask. */
const decidingSeat = (action: SecondaryFacts): string | null =>
  action.open
    ? (action.followers.find((follower) => follower.state === "waiting")?.seat ?? null)
    : action.primary;

/**
 * Whether the panel is the strategic action of the update. It is not while the viewer has a
 * choice that is not its part of the action: a reaction window, a decision of another card.
 */
export function showsStrategy(update: SessionUpdate, local: LocalState): boolean {
  const action = update.secondary;
  if (!action || local.draft) {
    return false;
  }
  const seat = update.viewer.role === "player" ? update.viewer.seat : null;
  return (
    !openChoice(update, local) ||
    !!local.follow?.editing ||
    (seat !== null && seat === decidingSeat(action))
  );
}

/**
 * The seats under a band: the seats that did something or decide, then the seats that wait, in
 * their order, as one line.
 */
function waitingInShort(primary: SeatRowView, followers: SeatRowView[]): BlockView[] {
  const waits = (row: SeatRowView) => row.status.sign === "·";
  const waiting = followers.filter(waits);
  return [
    { kind: "seats", rows: [primary, ...followers.filter((row) => !waits(row))] },
    ...(waiting.length
      ? [
          {
            kind: "note" as const,
            tone: "quiet" as const,
            text: `Then: ${waiting.map((row) => row.name).join(", ")}`,
          },
        ]
      : []),
  ];
}

const button = (
  label: string,
  intent: ActionButtonView["intent"],
  tone: ActionButtonView["tone"] = "quiet",
  more: Partial<ActionButtonView> = {},
): ActionButtonView => ({ label, tone, intent, ...more });

export function selectStrategy(
  update: SessionUpdate,
  local: LocalState,
  catalog: ExplorationCatalog | null = null,
  /** The printed names. Without it a faction is shown by its id. */
  printed: Catalog | null = null,
): FlowActionView {
  const action = update.secondary!;
  const seat = update.viewer.role === "player" ? update.viewer.seat : null;
  // A secondary that asks the seat nothing, as the game is now, is no draft: the row says so.
  const nothing =
    !!local.follow?.over &&
    !local.follow.answers.length &&
    !local.follow.end &&
    !local.follow.ready;
  const follow = nothing ? null : (local.follow ?? null);
  const choice = openChoice(update, local);
  // The choice, the note and the buttons of a decision are those of every decision.
  const inner = selectAction(update, local, catalog, printed);
  const deciding = decidingSeat(action);
  const name = (id: string) => (id === seat ? "You" : seatLabel(update, id, printed));
  /** The live game asks the viewer for its part of the action. */
  const live = !!choice && !follow?.editing;
  // The card is the heading, so the question is said in the row. The question of a secondary
  // says what it costs (pattern 3).
  const secondary = choice?.details?.kind === "strategy_secondary" ? choice.details : null;
  // An editor is one screen of several questions: its blocks say what it asks and costs.
  const several = openEditor(update, local) !== null;
  // An ability that asks in the middle of the action is the band on top of it (`interrupt`): the
  // row of the seat has nothing open, and the seats that wait are one line.
  const band = !!inner.interrupt;
  const asked: BlockView[] = band
    ? []
    : choice
      ? [
          ...(several
            ? []
            : [
                {
                  kind: "note" as const,
                  tone: "quiet" as const,
                  strong:
                    secondary?.costs_token === true
                      ? (secondaryCost(secondary) ?? undefined)
                      : undefined,
                  text: promptWords(choice, update, printed).text,
                },
              ]),
          ...inner.blocks,
        ]
      : [];
  const summary = (follow?.answers ?? []).map(sentence).join(" · ");
  const editor = (): BlockView[] =>
    choice
      ? asked
      : [
          follow?.problem
            ? { kind: "note", tone: "accent", text: follow.problem }
            : follow?.over
              ? { kind: "note", tone: "success", text: follow.end ?? "Nothing more to decide." }
              : { kind: "note", tone: "quiet", text: "The game checks the draft." },
        ];

  const primary: SeatRowView = {
    order: "P",
    seat: action.primary,
    name: name(action.primary),
    you: action.primary === seat,
    status: action.open
      ? { tone: "done", sign: "✓", label: "Resolved" }
      : {
          tone: "live",
          sign: "▶",
          label: action.primary === seat && live ? "Your decision" : "Deciding",
        },
    open: !action.open && action.primary === seat && live && !band ? asked : undefined,
  };
  const followers = action.followers.map((follower, index): SeatRowView => {
    const you = follower.seat === seat;
    const before = name(index ? action.followers[index - 1].seat : action.primary).replace(
      "You",
      "you",
    );
    const row = { order: `${index + 1}`, seat: follower.seat, name: name(follower.seat), you };
    if (follower.state === "followed") {
      return { ...row, status: { tone: "done", sign: "✓", label: "Followed" } };
    }
    if (follower.state !== "waiting") {
      return {
        ...row,
        status: { tone: "quiet", sign: "–", label: "Passed" },
        text: follower.state === "ineligible" ? "No command token in the strategy pool" : undefined,
      };
    }
    if (you && live) {
      return {
        ...row,
        status: { tone: "live", sign: "▶", label: "Your decision" },
        ...(band ? {} : { open: asked }),
      };
    }
    if (you && follow?.ready) {
      // A ready draft is closed and says what it will do. Only the viewer sees it.
      return {
        ...row,
        status: { tone: "draft", sign: "✎", label: "Draft ready" },
        text: `${summary} · not sent, resolves after ${before}`,
      };
    }
    if (you && follow) {
      return {
        ...row,
        status: { tone: "draft", sign: "✎", label: "Private draft" },
        text: summary || `Your seat is after ${before}.`,
        open: editor(),
      };
    }
    return {
      ...row,
      status:
        follower.seat === deciding
          ? { tone: "live", sign: "▶", label: "Deciding" }
          : { tone: "quiet", sign: "·", label: `Waits for ${before}` },
      text: you && nothing ? "Nothing to decide as the game is now" : undefined,
    };
  });

  // What every state of a draft has: the last answer of the live game is taken back, and the
  // game goes on by a step. Enter is the main button of the draft, so the step has no key here.
  const others = [
    ...(local.stepping && !local.error && !local.replaying
      ? [button("Step", { type: "simulate" })]
      : []),
    ...inner.footer.actions.filter((item) => item.intent.type === "undo"),
  ];
  const startOver = follow?.answers.length ? [button("Start over", { type: "startOver" })] : [];
  const drafting = !!follow && !live;
  const footer: FlowActionView["footer"] = !drafting
    ? {
        ...inner.footer,
        note:
          follow?.canSend && !local.planNote
            ? `Your draft: ${summary}. It was not marked ready.`
            : inner.footer.note,
        actions: [
          ...(follow?.canSend ? [button("Send draft", { type: "confirmApply" })] : []),
          ...inner.footer.actions,
        ],
      }
    : follow.ready
      ? {
          note: "Your draft waits for your seat. You can still change it.",
          error: false,
          actions: [...others, button("Change draft", { type: "flow", action: "change" })],
        }
      : choice
        ? {
            ...inner.footer,
            note: local.staged ? inner.footer.note : "Private until your seat is reached.",
            actions: [
              ...others.filter((item) => item.intent.type === "simulate"),
              ...startOver,
              ...inner.footer.actions,
            ],
          }
        : {
            note:
              follow.problem ??
              (follow.over ? "Private until your seat is reached." : "The game checks the draft."),
            error: !!follow.problem,
            actions: [
              ...others,
              ...startOver,
              ...(follow.over && !follow.problem
                ? [
                    button("Mark draft ready", { type: "flow", action: "ready" }, "primary", {
                      key: "Enter",
                    }),
                  ]
                : []),
            ],
          };
  const done = action.open && deciding === null;
  // The printed text of the card is behind hover: the card of this set, as the catalogue prints it.
  const help = strategyCardHelp(action.card, printed);
  return {
    ...inner,
    help,
    context: `Strategic action · ${name(action.primary)}`,
    badge: live ? inner.badge : null,
    heading: action.name,
    pill: live
      ? { tone: "live", label: "Your decision" }
      : drafting
        ? { tone: "draft", label: follow.ready ? "Draft ready" : "Private draft" }
        : done
          ? { tone: "done", label: "Resolved" }
          : null,
    contentKey: `strategy:${action.primary}:${action.card}`,
    blocks: band
      ? waitingInShort(primary, followers)
      : [{ kind: "seats", rows: [primary, ...followers] }],
    footer,
  };
}
