// The score of a public objective with what follows it (`details.secrets_held` and
// `details.secrets_max`, the engine's `ImperialHints`): the question of Imperial's primary (options of kind `objective`, a
// decline and `details.controls_mecatol`), and the window of its secondary (`yes` and `no`). The
// primary is one screen: the objectives that the game lets the seat score, as printed, and "Then",
// what the card does after it: a victory point with Mecatol Rex, else a secret objective. The
// main button answers the question by the id of its option (`Transport.applyDraft`).
import type { BlockView, MenuRowView } from "../../model";
import type { Catalog, Choice, ImperialHints, SessionUpdate } from "../wire";
import { chooseFollow, followWindow, isFollowWindow } from "./follow";
import { plain } from "./components";
import { humanizeId } from "./names";
import { type Editor, answered, hintFieldsOf } from "./editor";

/** What the player staged: the objective to score (by option id), or the answer of a window. */
export interface ScoringDraft {
  objective: string | null;
  follow: boolean | null;
}

export const NO_SCORING: ScoringDraft = { objective: null, follow: null };

interface Question {
  /** The objectives are chosen, or the window of a secondary. */
  mode: "score" | "follow";
  /** The objectives that can be scored, in the order of the options. */
  objectives: string[];
  /** The option that scores nothing. */
  decline: string | null;
  /** Null for a window. */
  mecatol: boolean | null;
  held: number;
  max: number;
}

function questionOf(choice: Choice): Question | null {
  const told = hintFieldsOf<ImperialHints>(choice, "secrets_held", "secrets_max");
  if (!told) {
    return null;
  }
  const { secrets_held: held, secrets_max: max } = told;
  const details = choice.details ?? {};
  if (isFollowWindow(choice)) {
    return { mode: "follow", objectives: [], decline: null, mecatol: null, held, max };
  }
  const objectives = choice.options.filter((option) => option.kind !== "decline");
  if (
    typeof details.controls_mecatol !== "boolean" ||
    !objectives.length ||
    objectives.some((option) => option.kind !== "objective")
  ) {
    return null;
  }
  return {
    mode: "score",
    objectives: objectives.map((option) => option.id),
    decline: choice.options.find((option) => option.kind === "decline")?.id ?? null,
    mecatol: details.controls_mecatol,
    held,
    max,
  };
}

export interface ScoringView {
  blocks: BlockView[];
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** The answers, by the ids of their options. Null: not yet. */
  answers: string[] | null;
}

const secrets = (held: number, max: number) => `Secret objectives ${held} → ${held + 1} of ${max}`;

/** The score of a public objective with Imperial, or its window. Null for any other question. */
export function selectScoring(
  update: SessionUpdate,
  choice: Choice,
  draft: ScoringDraft | null,
  printed: Catalog | null = null,
): ScoringView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const { held, max } = question;
  if (question.mode === "follow") {
    return followWindow(
      choice,
      draft?.follow ?? null,
      "Draw 1 secret objective",
      secrets(held, max),
      "draw 1 secret objective",
    );
  }
  const cards = new Map(printed?.records.public_objectives.map((card) => [card.id, card]));
  const name = (id: string) => cards.get(id)?.name ?? humanizeId(id);
  // The engine sends the worth of no option; the printed card has it.
  const worth = (id: string) => cards.get(id)?.points ?? null;
  const points = update.view.players.find((player) => player.id === choice.player)?.victory_points;
  const staged = question.objectives.find((id) => id === draft?.objective) ?? null;
  const gained = staged ? (worth(staged) ?? 0) : 0;
  const rows: MenuRowView[] = question.objectives.map((id, index) => {
    const more = worth(id);
    return {
      key: `${index + 1}`,
      title: name(id),
      text: cards.has(id) ? plain(cards.get(id)!.text) : undefined,
      state:
        typeof points === "number" && more !== null
          ? `Victory points ${points} → ${points + more}`
          : undefined,
      selected: id === staged,
      disabled: false,
      intent: { type: "chooseOption", option: id, group: "objective" },
    };
  });
  const before = typeof points === "number" ? points + gained : null;
  const then = question.mecatol
    ? {
        title: "Gain 1 victory point",
        subtitle: [
          "You control Mecatol Rex",
          ...(before !== null ? [`Victory points ${before} → ${before + 1}`] : []),
        ].join(" · "),
      }
    : {
        title: "Draw 1 secret objective",
        subtitle: `You do not control Mecatol Rex · ${secrets(held, max)}`,
      };
  const blocks: BlockView[] = [
    { kind: "menu", title: "Public objectives", rows },
    {
      kind: "card",
      title: "Then",
      rows: [{ icon: "card", title: [then.title], subtitle: then.subtitle }],
    },
  ];
  const problem = !staged && !question.decline ? "Choose a public objective." : null;
  return {
    blocks,
    note:
      problem ??
      (staged
        ? `Score ${name(staged)}, then ${then.title.toLowerCase()}`
        : `Score nothing, then ${then.title.toLowerCase()}`),
    problem,
    label: staged ? "Resolve primary" : "Resolve primary · score nothing",
    changed: staged !== null,
    answers: problem ? null : [staged ?? question.decline!],
  };
}

/** An objective is chosen, or the answer of the window; the same choice again takes it back. */
export function chooseScoring(
  choice: Choice,
  draft: ScoringDraft | null,
  group: string | undefined,
  option: string,
): ScoringDraft | null {
  const question = questionOf(choice);
  const staging = draft ?? NO_SCORING;
  if (question?.mode === "follow") {
    const chosen = chooseFollow(staging.follow, group, option);
    return chosen ? { ...staging, ...chosen } : null;
  }
  if (!question || group !== "objective" || !question.objectives.includes(option)) {
    return null;
  }
  return { ...staging, objective: staging.objective === option ? null : option };
}

/** Esc clears the objective, or the answer of the window. */
export function clearLastScoring(choice: Choice, draft: ScoringDraft | null): ScoringDraft | null {
  const question = questionOf(choice);
  if (!question || !draft) {
    return null;
  }
  const staged = question.mode === "follow" ? draft.follow !== null : draft.objective !== null;
  return staged ? NO_SCORING : null;
}

/** The score of a public objective as an editor (`editors.ts`). */
export const scoringEditor: Editor<ScoringDraft> = {
  view: (update, choice, draft, printed) => answered(selectScoring(update, choice, draft, printed)),
  option: chooseScoring,
  last: clearLastScoring,
};
