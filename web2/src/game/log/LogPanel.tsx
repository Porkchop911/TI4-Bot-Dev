import { useState, type ReactNode } from "react";
import type { ActionView, BlockView, HelpView, PillView, StepTabView } from "../../model";
import { Badge, GroupHeading, Hint, Trail, cx } from "../../ui";
import { Interrupt, Review, StepContent, pillBeyond } from "../action/ActionPanel";
import { ReadOnly } from "../context";
import { Blocks } from "../flows/Blocks";

/**
 * The log: what the others are doing and what they did, so that the viewer can decide. The
 * turns since the viewer's last turn come first (not there yet), then the action that is played
 * now, last and open: what the action panel shows, read-only, its parts in groups that open and
 * close. The decision of the viewer is the step sheet on the map; the Log only reads it.
 * Desktop has it under the player table, where it fills the column and scrolls inside itself;
 * a phone has it as the pane Log.
 */
export function LogPanel({
  action,
  status,
}: {
  action: ActionView;
  /** The status in the bar of the game. The pill of the heading does not say it again. */
  status: string;
}) {
  return (
    <section aria-label="Log" className="min-h-0 flex-1 overflow-y-auto px-3 py-2.5">
      <p className="text-sm text-muted">No turns since your last turn</p>
      <ReadOnly>
        <CurrentAction
          // Another action starts with its own groups open.
          key={action.kind === "tactical" ? action.context : `${action.heading}·${action.context}`}
          view={action}
          status={status}
        />
      </ReadOnly>
    </section>
  );
}

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
 * closed: they are the decision of a seat, and the viewer's own is in the step sheet.
 */
function CurrentAction({ view, status }: { view: ActionView; status: string }) {
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

  const window = view.interrupt && part("window", view.interrupt.label, null, true);
  return (
    <article
      aria-label="Current action"
      className="mt-2.5 overflow-hidden rounded-lg border border-line bg-surface/40"
    >
      <Heading heading={heading} context={context} pill={pill} help={help} />
      {trail.length > 0 && (
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
        view.tabs.length ? (
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
        <Parts blocks={[...view.blocks, ...view.closing]} part={part} />
      )}
    </article>
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
function Parts({
  blocks,
  part,
}: {
  blocks: BlockView[];
  part: (
    id: string,
    name: ReactNode,
    count: ReactNode,
    initially: boolean,
  ) => { open: boolean; heading: ReactNode };
}) {
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
