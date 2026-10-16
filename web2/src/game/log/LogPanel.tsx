import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import {
  informs,
  type ActionView,
  type BlockView,
  type HelpView,
  type LogItemView,
  type LogView,
  type PillView,
  type StepTabView,
} from "../../model";
import { Badge, GroupHeading, Hint, Trail, cx } from "../../ui";
import { Interrupt, Review, StepContent, pillBeyond } from "../action/ActionPanel";
import { ReadOnly } from "../context";
import { Blocks } from "../flows/Blocks";
import { LogHistory } from "./LogHistory";

/** How near the end the Log counts as at its end, in pixels: it follows what is new. */
const AT_END = 24;

/**
 * The log: what the others are doing and what they did, so that the viewer can decide. The
 * turns since the viewer's last turn come first, one row each, which opens to its details. The
 * action that is played now is the content of the live entry (the turn that is played now, or
 * the phase in play), under its details and open: what the action panel shows, read-only, its
 * parts in groups that open and close. With no live entry it is an entry of its own, last. While
 * the viewer decides, the decision is the step sheet on the map, and the Log has only what
 * informs (`BlockRole`): the other seats, what happened. Desktop has it under the player table,
 * where it fills the column and scrolls inside itself; a phone has it as the pane Log, which
 * scrolls the same way. The newest is at the end: the Log starts there, and follows what is new
 * while it is at its end; else it stays where the viewer scrolled.
 */
export function LogPanel({
  log,
  action,
  status,
  asked,
}: {
  log: LogView;
  action: ActionView;
  /** The status in the toolbar. The pill of the heading does not say it again. */
  status: string;
  /** The viewer has a decision open: the step sheet has it, and the Log what only informs. */
  asked: boolean;
}) {
  const scroller = useRef<HTMLElement>(null);
  const atEnd = useRef(true);
  // The rows that the viewer opened or closed; an entry that left the Log takes its row with it.
  const [toggled, setToggled] = useState<ReadonlyMap<string, boolean>>(new Map());
  const isOpen = (entry: LogItemView) => toggled.get(entry.id) ?? entry.live;
  const last = log.entries.at(-1);
  const live = last?.live ? last : null;
  const parts = partsOf(action, asked, !!live);
  const now = parts && (
    <ReadOnly>
      <CurrentAction
        // Another action starts with its own groups open.
        key={action.kind === "tactical" ? action.context : `${action.heading}·${action.context}`}
        view={action}
        status={status}
        parts={parts}
        framed={!live}
      />
    </ReadOnly>
  );
  // biome-ignore lint/correctness/useExhaustiveDependencies: what is new is the trigger; a row that the viewer opens is not
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && atEnd.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [log, action]);
  return (
    <section
      ref={scroller}
      aria-label="Log"
      className="min-h-0 flex-1 overflow-y-auto py-2.5 phone:bg-board"
      onScroll={(event) => {
        const element = event.currentTarget;
        atEnd.current = element.scrollHeight - element.scrollTop - element.clientHeight <= AT_END;
      }}
    >
      <LogHistory
        view={log}
        isOpen={isOpen}
        now={live ? now : null}
        onToggle={(entry) =>
          setToggled((before) => {
            const ids = new Set(log.entries.map((item) => item.id));
            const next = new Map([...before].filter(([id]) => ids.has(id)));
            next.set(entry.id, !isOpen(entry));
            return next;
          })
        }
      />
      {!live && now && <div className="px-3">{now}</div>}
    </section>
  );
}

/** What the Log shows of the action that is played now. Null: nothing. */
interface NowParts {
  /** The heading line: the heading, its context, its state, its rules. */
  heading: boolean;
  /** The band of a window. */
  window: boolean;
  /** The blocks of a strategic or component action, a turn or a wait. */
  blocks: BlockView[];
  /** The step of a tactical action that is in view. */
  step: boolean;
}

/**
 * The parts of the action that the Log has. While the viewer decides, the step sheet has the
 * heading, the band, the decision and its rules: the Log has the blocks and the step that only
 * inform. With no decision of the viewer open, it has everything, read-only.
 */
function partsOf(view: ActionView, asked: boolean, merged: boolean): NowParts | null {
  if (!asked) {
    return {
      heading: true,
      window: !!view.interrupt,
      blocks: view.kind === "flow" ? [...view.blocks, ...view.closing] : [],
      step: view.kind === "tactical",
    };
  }
  const blocks = view.kind === "flow" ? view.blocks.filter(informs) : [];
  const step = view.kind === "tactical" && view.task.role === "inform";
  // An entry of its own says what it is; in the live entry the row says it.
  return blocks.length || step ? { heading: !merged, window: false, blocks, step } : null;
}

/** A part of the action: a group that opens and closes, open at first or not. */
type Part = (
  id: string,
  name: ReactNode,
  count: ReactNode,
  initially: boolean,
) => { open: boolean; heading: ReactNode };

/** The name of a part of the action: the group that holds a block. Null: the block is one line. */
function partName(block: BlockView): string | null {
  switch (block.kind) {
    case "note":
    case "source":
      return null;
    case "seats":
      return "Seats";
    case "gauges":
      return "Limits";
    case "deal":
      return "Deal";
    case "menu":
      return block.title ?? "Choices";
    case "techs":
    case "card":
    case "draft":
      return block.title;
  }
}

/** The block without the name that the heading of its group says. */
function untitled(block: BlockView): BlockView {
  switch (block.kind) {
    case "menu":
      return { ...block, title: undefined };
    case "techs":
    case "card":
    case "draft":
      return { ...block, title: "" };
    default:
      return block;
  }
}

/** The sign of the state of a step, so that the colour is not its only carrier. */
const STEP_SIGN: Record<StepTabView["status"], string> = {
  future: "·",
  skipped: "–",
  done: "✓",
  decision: "▶",
  battle: "▶",
  "needs-review": "!",
  draft: "✎",
};

const STEP_TONE: Record<StepTabView["status"], string> = {
  future: "text-faint",
  skipped: "text-faint",
  done: "text-green",
  decision: "text-accent",
  battle: "text-accent",
  "needs-review": "text-red",
  draft: "text-gold",
};

/**
 * The action that is played now, read-only: its heading with the state, then its parts. A part
 * opens and closes; the part in progress is open. A decision list and the technology tree are
 * closed: they are the decision of a seat, and the viewer's own is in the step sheet. An entry of
 * its own (`framed`) is a card, and a tactical action has a line for each step; in the live entry
 * of the history, whose details tell the steps that were played, it has the step in view only.
 */
function CurrentAction({
  view,
  status,
  parts,
  framed,
}: {
  view: ActionView;
  status: string;
  parts: NowParts;
  framed: boolean;
}) {
  // What the viewer opened or closed; every other part has its default.
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
  const part = (id: string, name: ReactNode, count: ReactNode, initially: boolean) => {
    const open = toggled[id] ?? initially;
    return {
      open,
      heading: (
        <GroupHeading
          open={open}
          onToggle={() => setToggled((now) => ({ ...now, [id]: !open }))}
          name={name}
          count={count}
          className="px-3 py-1 text-sm"
        />
      ),
    };
  };
  const tactical = view.kind === "tactical";
  const heading = tactical ? (view.context ?? view.task.title) : view.heading;
  const context = tactical ? null : view.context;
  // The pill of a tactical action is of its step: the group of the step says it.
  const pill = tactical ? null : pillBeyond(view.pill, status);
  const help = tactical ? view.task.help : view.help;
  const trail = tactical ? [] : view.trail;

  const window = parts.window && view.interrupt && part("window", view.interrupt.label, null, true);
  return (
    <article
      aria-label="Current action"
      className={
        framed
          ? "mt-2.5 overflow-hidden rounded-lg border border-line bg-surface/40"
          : "bg-well/40 pl-5 phone:pl-3"
      }
    >
      {parts.heading && <Heading heading={heading} context={context} pill={pill} help={help} />}
      {parts.heading && trail.length > 0 && (
        <div className="border-b border-line px-3 py-1.5">
          <Trail items={trail} />
        </div>
      )}
      {window && (
        <div>
          {window.heading}
          {window.open && <Interrupt view={view.interrupt!} className="border-b-0" />}
        </div>
      )}
      {tactical ? (
        !parts.step ? null : view.tabs.length && !framed ? (
          // The details of the live entry tell the steps that were played: the step in view.
          <StepPart view={view} status={status} part={part} />
        ) : view.tabs.length ? (
          view.tabs.map((tab, index) => {
            const sign = (
              <span className={cx("w-3 text-center", STEP_TONE[tab.status])}>
                {STEP_SIGN[tab.status]}
              </span>
            );
            // Only the step in view has its content: the others are a line with their state.
            if (index !== view.selected) {
              return (
                <div
                  key={tab.name}
                  className={cx(
                    "flex items-baseline gap-2 border-b border-line px-3 py-1 text-sm",
                    tab.status === "future" ? "text-faint" : "font-strong",
                  )}
                >
                  <span aria-hidden="true" className="invisible">
                    ▸
                  </span>
                  {sign}
                  {tab.name}
                  <span className="text-xs font-normal text-muted">{tab.caption}</span>
                </div>
              );
            }
            const step = part(
              `step:${index}`,
              <span className="flex items-baseline gap-2">
                {sign}
                {tab.name}
              </span>,
              [tab.caption, pillBeyond(view.task.pill, status)?.label].filter(Boolean).join(" · "),
              true,
            );
            return (
              <div key={tab.name}>
                {step.heading}
                {step.open && <StepBody view={view} />}
              </div>
            );
          })
        ) : (
          // A production of a strategic action has no steps: its content is the one part.
          <StepBody view={view} />
        )
      ) : (
        <Parts blocks={parts.blocks} part={part} />
      )}
    </article>
  );
}

/** The step of a tactical action that is in view, as one part. */
function StepPart({
  view,
  status,
  part,
}: {
  view: Extract<ActionView, { kind: "tactical" }>;
  status: string;
  part: Part;
}) {
  const tab = view.tabs[view.selected];
  const step = part(
    "step",
    tab?.name ?? view.task.title,
    [tab?.caption, pillBeyond(view.task.pill, status)?.label].filter(Boolean).join(" · "),
    true,
  );
  return (
    <div>
      {step.heading}
      {step.open && <StepBody view={view} />}
    </div>
  );
}

function StepBody({ view }: { view: Extract<ActionView, { kind: "tactical" }> }) {
  return (
    <div className="space-y-2 border-b border-line px-3 py-2">
      {view.task.review && <Review view={view.task.review} />}
      <StepContent view={view.task.content} />
      <Blocks blocks={view.closing} />
    </div>
  );
}

/** The blocks of an action: a block of one line as it is, any other in a group of its own. */
function Parts({ blocks, part }: { blocks: BlockView[]; part: Part }) {
  return blocks.map((block, index) => {
    const name = partName(block);
    if (name === null) {
      return (
        <div key={index} className="border-b border-line px-3 py-2">
          <Blocks blocks={[block]} />
        </div>
      );
    }
    const group = part(
      `${block.kind}:${index}`,
      name,
      null,
      block.kind !== "menu" && block.kind !== "techs",
    );
    return (
      <div key={index}>
        {group.heading}
        {group.open && (
          <div className="border-b border-line px-3 py-2">
            <Blocks blocks={[untitled(block)]} />
          </div>
        )}
      </div>
    );
  });
}

/** The heading of the current action in one line: the heading, its context, its state, its rules. */
function Heading({
  heading,
  context,
  pill,
  help,
}: {
  heading: string;
  context: string | null;
  pill: PillView | null;
  help: HelpView[];
}) {
  return (
    <div className="flex min-w-0 items-center gap-x-2 border-b border-line px-3 py-1.5">
      <h3 className="min-w-0 truncate font-semibold" title={heading}>
        {heading}
      </h3>
      {context && (
        <span className="min-w-0 shrink-[2] truncate text-sm text-muted" title={context}>
          {context}
        </span>
      )}
      {pill && (
        <Badge tone={pill.tone} className="shrink-0">
          {pill.label}
        </Badge>
      )}
      {help.length > 0 && (
        <span className="shrink-0">
          <Hint label={`Rules: ${heading}`}>
            {help.map((item) => (
              <p key={item.title}>
                <strong className="block">{item.title}</strong>
                {item.text}
              </p>
            ))}
          </Hint>
        </span>
      )}
    </div>
  );
}
