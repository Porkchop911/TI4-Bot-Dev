// The activation, the movement, the landings and the production of a tactical action, on the
// screens of these steps. The other steps of the action are still decisions in the list
// (`selectAction`).
import type {
  ActionButtonView,
  ActivationView,
  FlowActionView,
  FooterView,
  PillView,
  StepContentView,
  StepTabView,
  TacticalActionView,
} from "../../model";
import { plural } from "../../model";
import { landed } from "../landingDraft";
import { isEmpty, overrun, totals } from "../productionDraft";
import type {
  ActivationFacts,
  Catalog,
  CombatView as CombatState,
  ExplorationCatalog,
  InvasionFacts,
  MovementFacts,
  ProductionFacts,
  SessionUpdate,
  TacticalFacts,
} from "../wire";
import { type LocalState, openChoice } from "./action";
import { selectCombat } from "./combat";
import { selectInvasion } from "./invasion";
import { selectProduction } from "./production";
import {
  movementTrail,
  selectMovement,
  selectMovementResult,
  systemLabel,
  tilesOf,
} from "./movement";
import { ANOMALIES, WORMHOLES, factionName } from "./names";
import { reviewRow } from "./review";

const STEPS = ["Activation", "Movement", "Space combat", "Invasion", "Production"];

/**
 * The space combat of the action (`BoardView.combat`). It has no facts of a step: the state of
 * the combat, which every seat sees, and the open question of the viewer as the decision of
 * `selectAction` (a list, or the band of a reaction window).
 */
export interface CombatStep {
  kind: "combat";
  combat: CombatState;
  flow: FlowActionView;
  /** The viewer is asked a question of the game now. */
  asked: boolean;
}

/** What the screen of a tactical action is made of: the facts of a step, or the space combat. */
export type TacticalStep = TacticalFacts | CombatStep;

/**
 * The facts of the open choice, when it is the activation, the movement, the landings or the
 * production of the viewer.
 */
export function tacticalFacts(update: SessionUpdate, local: LocalState): TacticalFacts | null {
  const choice = openChoice(update, local);
  const subtype = choice?.context?.subtype;
  const facts = update.tactical;
  // An activation is any choice of systems to activate: also one that places no command token.
  const activating =
    !!choice?.options.length && choice.options.every((option) => option.kind === "activate");
  return (facts?.kind === "activation" && activating) ||
    (facts?.kind === "movement" && subtype === "movement_step") ||
    (facts?.kind === "invasion" && subtype === "commit_ground_forces") ||
    (facts?.kind === "production" && subtype === "produce_unit")
    ? facts
    : null;
}

const UNDO: ActionButtonView = { label: "Undo", tone: "quiet", intent: { type: "undo" } };

/** The tab of each step that a draft stages. */
const TAB = { movement: 1, invasion: 3, production: 4 };

/**
 * The footer of a step of a draft. Every change is recorded at once, so nothing is sent: the
 * button records a step with nothing staged, or goes on to the step that the game comes to.
 */
function draftFooter(
  local: LocalState,
  nothing: string,
  note: string | null = null,
  error = false,
): FooterView {
  const info = local.draft;
  const main = (label: string, intent: ActionButtonView["intent"]): ActionButtonView => ({
    label,
    tone: "primary",
    key: "Enter",
    arrow: true,
    intent,
  });
  return {
    note: local.planNote?.text ?? note ?? info?.ends ?? "Private to you until you apply.",
    error: !!local.planNote?.error || error,
    actions: !info?.recorded
      ? [main(nothing, { type: "commitEdit" })]
      : info.next
        ? [
            main(`Go to ${STEPS[TAB[info.next]].toLowerCase()}`, {
              type: "selectStep",
              step: TAB[info.next],
            }),
          ]
        : [],
  };
}

function activation(
  update: SessionUpdate,
  local: LocalState,
  facts: ActivationFacts,
): { content: ActivationView; footer: FooterView; target: string | null } {
  const tiles = tilesOf(update);
  const reach = facts.systems.find((item) => item.system === local.staged);
  const tile = reach && tiles.get(reach.system);
  const label = reach ? systemLabel(tiles, reach.system) : null;
  // The recorded activation of a draft is open: another system replaces it.
  const editing = local.draft?.editing ?? null;
  return {
    target: label,
    content: {
      kind: "activation",
      editing: true,
      draft: !!local.draft,
      tacticPool: [facts.tactic_tokens, facts.tactic_tokens - 1],
      costsToken: facts.costs_token,
      systems: facts.systems.map((item) => ({
        id: item.system,
        label: systemLabel(tiles, item.system),
      })),
      active: null,
      chosen: reach
        ? {
            system: reach.system,
            label: label!,
            facts:
              [
                ...(tile?.anomalies ?? []).map((name) =>
                  name in ANOMALIES ? name.charAt(0).toUpperCase() + name.slice(1) : name,
                ),
                ...(tile?.wormholes ?? []).map((name) => `${WORMHOLES[name] ?? name} wormhole`),
                plural(tile?.planets?.length ?? 0, "planet"),
              ].join(" · ") || "Empty space",
            commandToken: false,
            shipsInRange: reach.ships,
            originsInRange: reach.origins,
            production: reach.production ?? 0,
            cannon: reach.cannon ?? 0,
          }
        : null,
    },
    footer: {
      note:
        (editing && reach?.system === editing
          ? "Recorded. Choose another system on the map."
          : label && `Selected: ${label}`) || "Choose a system on the map.",
      error: false,
      actions: [
        ...(local.canUndo ? [UNDO] : []),
        ...(editing
          ? [
              {
                label: "Cancel edits",
                tone: "quiet" as const,
                intent: { type: "cancelEdit" as const },
              },
            ]
          : []),
        ...(reach
          ? [
              {
                label: "Clear",
                tone: "quiet" as const,
                key: "Escape",
                intent: { type: "clearChoice" as const },
              },
            ]
          : []),
        {
          label: "Activate system",
          tone: "primary",
          disabled: !reach,
          key: "Enter",
          arrow: true,
          intent: { type: "commitEdit" },
        },
      ],
    },
  };
}

function movement(
  update: SessionUpdate,
  local: LocalState,
  facts: MovementFacts,
  printed: Catalog | null,
): { content: StepContentView; footer: FooterView; target: string } {
  const tiles = tilesOf(update);
  const target = systemLabel(tiles, facts.active);
  if (local.step === 0) {
    // The recorded activation, read-only. The token of this activation is already spent.
    const pool =
      update.view.players.find((player) => player.id === update.pending_choice?.choice.player)
        ?.tactic_tokens ?? 0;
    return {
      target,
      content: {
        kind: "activation",
        editing: false,
        draft: !!local.draft,
        chosen: null,
        systems: [],
        active: { system: facts.active, name: tiles.get(facts.active)?.label ?? "System" },
        tacticPool: [pool + 1, pool],
        costsToken: true,
      },
      footer: {
        note: "Viewing activation. The action is at movement.",
        error: false,
        actions: [
          {
            label: "Continue to movement",
            tone: "primary",
            key: "Enter",
            arrow: true,
            intent: { type: "returnToCurrent" },
          },
        ],
      },
    };
  }
  const nothing = Object.keys(local.movement).length === 0;
  const content = selectMovement(
    update,
    facts,
    local.movement,
    local.handled,
    local.odds ?? null,
    local.rolls,
    printed,
  );
  if (local.draft) {
    return { target, content, footer: draftFooter(local, "Move nothing") };
  }
  return {
    target,
    content,
    footer: {
      note: local.planNote?.text ?? "This commits to the live game.",
      error: !!local.planNote?.error,
      actions: [
        ...(local.canUndo ? [UNDO] : []),
        ...(nothing
          ? []
          : [
              {
                label: "Reset selection",
                key: "Escape",
                intent: { type: "cancelEdit" as const },
              },
            ]),
        {
          label: nothing ? "Move nothing" : "Move fleet",
          tone: "primary",
          key: "Enter",
          arrow: true,
          intent: { type: "commitEdit" },
        },
      ],
    },
  };
}

function invasion(
  update: SessionUpdate,
  local: LocalState,
  facts: InvasionFacts,
  catalog: ExplorationCatalog | null,
  printed: Catalog | null,
): { content: StepContentView; footer: FooterView; target: string } {
  const nothing = landed(local.landing) === 0;
  const content = selectInvasion(
    update,
    facts,
    local.landing,
    local.landingKind,
    catalog,
    local.groundOdds ?? {},
    printed,
  );
  if (local.draft) {
    return {
      target: systemLabel(tilesOf(update), facts.active),
      content,
      footer: draftFooter(local, "Land nothing"),
    };
  }
  return {
    target: systemLabel(tilesOf(update), facts.active),
    content,
    footer: {
      note: local.planNote?.text ?? "This commits to the live game.",
      error: !!local.planNote?.error,
      actions: [
        ...(local.canUndo ? [UNDO] : []),
        ...(nothing
          ? []
          : [
              {
                label: "Reset selection",
                key: "Escape",
                intent: { type: "cancelEdit" as const },
              },
            ]),
        {
          label: nothing ? "Land nothing" : "Commit ground forces",
          tone: "primary",
          key: "Enter",
          arrow: true,
          intent: { type: "commitEdit" },
        },
      ],
    },
  };
}

function production(
  update: SessionUpdate,
  local: LocalState,
  facts: ProductionFacts,
  printed: Catalog | null,
): { content: StepContentView; footer: FooterView; target: string } {
  const stage = local.productionStage;
  const content = selectProduction(
    update,
    facts,
    local.production,
    stage,
    printed,
    local.standing?.response ?? null,
  );
  const { units, cost, paid } = totals(facts, local.production);
  // The limits are soft in the sheet. A build over one blocks the main button, as a payment
  // that is short does: the game would refuse it.
  const over = overrun(facts, local.production);
  const problems = [
    over.short > 0 ? `${plural(over.short, "resource")} short` : null,
    over.capacity > 0 ? `${plural(over.capacity, "unit")} over the production limit` : null,
  ].filter((text) => text !== null);
  const blocked = problems.length > 0;
  const main = (label: string, disabled = false): ActionButtonView => ({
    label,
    tone: "primary",
    key: "Enter",
    arrow: true,
    disabled,
    intent: { type: "commitEdit" },
  });
  const reset: ActionButtonView[] = isEmpty(local.production)
    ? []
    : [{ label: "Reset selection", intent: { type: "cancelEdit" } }];
  const paying = stage === "pay" && units > 0 && cost > 0;
  if (local.draft) {
    // A build that is not paid is kept in the draft; the game is asked for it when it is paid.
    const open = units > 0 && paid < cost;
    const footer = draftFooter(
      local,
      "Produce nothing",
      blocked
        ? problems.join(" · ")
        : open
          ? "The production is in the draft when it is paid."
          : null,
      blocked,
    );
    return {
      target: systemLabel(tilesOf(update), facts.system),
      content,
      footer: {
        ...footer,
        payment: paying ? content.payment : undefined,
        actions: paying
          ? [
              { label: "Back to build", key: "Escape", intent: { type: "clearChoice" } },
              ...(open ? [] : footer.actions),
            ]
          : open
            ? [main(`Pay ${plural(cost, "resource")}`, blocked)]
            : footer.actions,
      },
    };
  }
  return {
    target: systemLabel(tilesOf(update), facts.system),
    content,
    footer: {
      note:
        local.planNote?.text ?? (blocked ? problems.join(" · ") : "This commits to the live game."),
      error: !!local.planNote?.error || blocked,
      // The open payment, as one line. Its controls are on the board.
      payment: paying ? content.payment : undefined,
      actions: [
        ...(local.canUndo ? [UNDO] : []),
        ...reset,
        ...(paying
          ? [
              {
                label: "Back to build",
                key: "Escape",
                intent: { type: "clearChoice" as const },
              },
              main("Produce units", paid < cost),
            ]
          : [
              main(
                units === 0
                  ? "Produce nothing"
                  : cost > 0
                    ? `Pay ${plural(cost, "resource")}`
                    : "Produce units",
                blocked,
              ),
            ]),
      ],
    },
  };
}

const CURRENT = { activation: 0, movement: 1, combat: 2, invasion: 3, production: 4 };

/**
 * The movement of the action in Live, read-only, while a later step is open: what the dice of
 * the movement did.
 */
function movementResult(
  update: SessionUpdate,
  local: LocalState,
  active: string,
  current: number,
  printed: Catalog | null,
): { content: StepContentView; footer: FooterView; target: string } {
  const next = STEPS[current].toLowerCase();
  return {
    target: systemLabel(tilesOf(update), active),
    content: selectMovementResult(update, active, local.rolls ?? [], printed),
    footer: {
      note: `Viewing movement. The action is at ${next}.`,
      error: false,
      actions: [
        {
          label: `Continue to ${next}`,
          tone: "primary",
          key: "Enter",
          arrow: true,
          intent: { type: "returnToCurrent" },
        },
      ],
    },
  };
}

export function selectTactical(
  update: SessionUpdate,
  local: LocalState,
  facts: TacticalStep,
  catalog: ExplorationCatalog | null = null,
  /** The printed names. Without it a faction is shown by its id and a unit by its base unit. */
  printed: Catalog | null = null,
): TacticalActionView {
  const current = CURRENT[facts.kind];
  // In Live the movement of the action stays open to read while a later step is open.
  const reading =
    !local.draft && local.step === 1 && current > 1 && !!update.view.board.active_system;
  const selected = facts.kind === "movement" && local.step === 0 ? 0 : reading ? 1 : current;
  const view: {
    content: StepContentView;
    footer: FooterView;
    target: string | null;
    pill?: PillView;
  } = reading
    ? movementResult(update, local, update.view.board.active_system!, current, printed)
    : facts.kind === "combat"
      ? selectCombat(update, facts.combat, facts.flow, facts.asked, printed)
      : facts.kind === "activation"
        ? activation(update, local, facts)
        : facts.kind === "movement"
          ? movement(update, local, facts, printed)
          : facts.kind === "invasion"
            ? invasion(update, local, facts, catalog, printed)
            : production(update, local, facts, printed);
  const units = landed(local.landing);
  const built = facts.kind === "production" ? totals(facts, local.production).units : 0;
  const active =
    facts.kind === "production"
      ? facts.system
      : facts.kind === "combat"
        ? facts.combat.system_id
        : "active" in facts
          ? facts.active
          : "";
  // The seat of the action. In a space combat the viewer may be another seat, or none.
  const seat =
    facts.kind === "combat"
      ? (update.view.active_player ?? facts.combat.attacker)
      : (update.pending_choice?.choice.player ?? "");
  const faction = factionName(
    update.view.players.find((player) => player.id === seat)?.faction ?? seat,
    printed,
  );
  // A production of a strategic action (Warfare's secondary) is no tactical action: it has no
  // steps, and the card is behind "?" of its heading, as on the list of the seats.
  const strategic =
    facts.kind === "production" && !local.draft && !update.view.board.active_system
      ? update.secondary
      : null;
  // A tactical action of the primary of a strategy card (Warfare of Thunder's Edge) names the
  // card, with its primary behind "?".
  const primary =
    !strategic && !local.draft && update.secondary?.primary === seat ? update.secondary : null;
  const card =
    (strategic ?? primary)
      ? printed?.records.strategy_cards.find((item) => item.id === (strategic ?? primary)!.card)
      : undefined;
  const ships = Object.keys(local.movement).length;
  // A draft opens every step that the game came to with it, also the steps after the one in view.
  const reached = (local.draft?.reached ?? []).map((stage) => TAB[stage]);
  // The step whose row marks the entry of the draft that no longer fits.
  const review = local.draft?.review ?? null;
  const reviewTab = review ? (review.at === null ? 0 : TAB[review.at]) : null;
  const tabs = STEPS.map((name, step): StepTabView => {
    const status: StepTabView["status"] =
      step === reviewTab
        ? "needs-review"
        : step < current
          ? step === 2 && local.draft
            ? "skipped"
            : "done"
          : step === current
            ? facts.kind === "combat"
              ? "battle"
              : "decision"
            : reached.includes(step)
              ? "draft"
              : "future";
    return {
      name,
      status,
      caption:
        step === reviewTab
          ? // The pill of the step says it while the step is in view.
            step === selected
            ? ""
            : "Needs review"
          : step === 0 && facts.kind !== "activation"
            ? `#${active}`
            : step === 1 && current === 1 && ships
              ? plural(ships, "ship")
              : step === 2 && facts.kind === "combat"
                ? `Round ${facts.combat.round}`
                : step === 3 && current === 3 && units
                  ? plural(units, "unit")
                  : step === 4 && current === 4 && built
                    ? plural(built, "unit")
                    : "",
    };
  });
  return {
    kind: "tactical",
    title: strategic ? "Strategic action" : "Tactical action",
    subtitle: strategic
      ? `${strategic.name} · ${view.target ?? ""} · ${faction}`
      : `${primary ? `${primary.name} · ` : ""}${view.target ?? "No system chosen"} · ${faction}`,
    badge: local.draft
      ? { tone: "draft", label: "Private draft" }
      : { tone: "live", label: "In progress" },
    past: null,
    tabs: strategic ? [] : tabs,
    selected,
    current,
    contentKey: `tactical:${selected}`,
    uncommitted: null,
    // A reaction window during the combat is the band on top of the step.
    interrupt: facts.kind === "combat" ? facts.flow.interrupt : null,
    closing: [],
    task: {
      step: selected,
      title: strategic?.name ?? STEPS[selected],
      pill: local.draft
        ? local.draft.editing
          ? { tone: "draft", label: "Editing" }
          : selected === reviewTab
            ? { tone: "alert", label: "Needs review" }
            : selected !== current
              ? { tone: "done", label: "Drafted" }
              : local.draft.boundary
                ? { tone: "draft", label: "Preview ends here" }
                : { tone: "draft", label: "Preparing" }
        : selected !== current
          ? { tone: "done", label: "Resolved" }
          : view.pill
            ? view.pill
            : local.remaining === null && local.planNote && !local.planNote.error
              ? {
                  tone: "live",
                  label: `Your decision · ${facts.kind === "invasion" ? "landings go" : facts.kind === "production" ? "production goes" : "movement goes"} on`,
                }
              : { tone: "live", label: "Your decision" },
      trail:
        selected !== 1
          ? []
          : reading
            ? movementTrail(local.rolls ?? [], false, 0, null)
            : facts.kind === "movement" && view.content.kind === "movement"
              ? movementTrail(local.rolls ?? [], true, view.content.riftRolls, facts.cannon ?? [])
              : [],
      // The recorded activation of a draft can be changed; the game checks the later steps again.
      edit:
        local.draft && facts.kind === "movement" && selected === 0
          ? {
              label: "Edit activation",
              step: 0,
              disabled: local.draft.waiting,
              hint: local.draft.waiting ? "The game checks the draft" : undefined,
            }
          : null,
      review:
        review && selected === reviewTab && !local.draft?.editing
          ? reviewRow(
              update,
              facts.kind === "combat" ? null : facts,
              review.entry,
              review.system,
              printed,
            )
          : null,
      help: card
        ? [
            update.secondary?.primary === seat && !update.secondary.open
              ? { title: `${card.name} · Primary`, text: card.primary_text }
              : { title: `${card.name} · Secondary`, text: card.secondary_text },
          ].filter((item) => item.text)
        : [],
      content: view.content,
      footer: view.footer,
    },
  };
}
