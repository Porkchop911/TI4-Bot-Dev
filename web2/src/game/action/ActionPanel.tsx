import { payState } from "../steps/PaymentList";
import { useEffect, useRef, type ReactNode } from "react";
import type {
  ActionView,
  FooterView,
  HelpView,
  InterruptView,
  PillView,
  StepContentView,
  TaskView,
} from "../../model";
import { Badge, Button, Eyebrow, Hint, StepTabs, Trail, cx, type TrailItem } from "../../ui";
import { ActionButton, useDispatch } from "../context";
import { Blocks } from "../flows/Blocks";
import { useLink } from "../link";
import { ActivationStep } from "../steps/ActivationStep";
import { InvasionStep, SpaceCombatStep } from "../steps/BattleSteps";
import { MovementStep } from "../steps/MovementStep";
import { ProductionStep } from "../steps/ProductionStep";

function StepContent({ view }: { view: StepContentView }) {
  switch (view.kind) {
    case "activation":
      return <ActivationStep view={view} />;
    case "movement":
      return <MovementStep view={view} />;
    case "combat":
      return <SpaceCombatStep view={view} />;
    case "invasion":
      return <InvasionStep view={view} />;
    case "production":
      return <ProductionStep view={view} />;
  }
}

/** A reaction window on top of the open action: the trigger, and the cards that can be played now. */
function Interrupt({ view }: { view: InterruptView }) {
  const { linked, props } = useLink(view.link);
  return (
    <div
      role="group"
      aria-label={view.eyebrow}
      className={cx(
        "border-b border-cyan/30 bg-cyan/[.05] px-5 py-2.5 phone:px-3",
        linked && "bg-cyan/[.09]",
      )}
      {...props}
    >
      <div className="mb-1.5 flex items-center justify-between gap-3">
        <Eyebrow className="text-cyan">{view.eyebrow}</Eyebrow>
        <span className="flex gap-1.5">
          {view.actions.map((action) => (
            <ActionButton key={action.label} view={action} size="sm" />
          ))}
        </span>
      </div>
      <Blocks blocks={view.blocks} />
    </div>
  );
}

/** What the main button does, and the main button. A phone shows it under every pane. */
export function ActionFooter({ view: footer }: { view: FooterView }) {
  // Nothing to say and nothing to send: the footer takes no room.
  if (!footer.note && !footer.payment && !footer.actions.length) {
    return null;
  }
  return (
    <footer className="flex items-center justify-between gap-[15px] border-t border-line bg-surface px-5 py-2.5 phone:flex-col phone:items-stretch phone:gap-1.5 phone:px-3 phone:py-2">
      <span className="flex min-w-0 flex-wrap items-baseline gap-x-4 text-sm">
        {footer.payment && (
          // The open payment in one line. Its controls are on the board.
          <span className="whitespace-nowrap text-muted phone:whitespace-normal">
            <strong className="font-semibold text-text">Payment</strong>{" "}
            {footer.payment.summary || "nothing staged"} ·{" "}
            <span className={cx("font-semibold tabular-nums", payState(footer.payment).tone)}>
              {footer.payment.paid} / {footer.payment.cost}
              {footer.payment.paid > footer.payment.cost && ` · ${payState(footer.payment).label}`}
            </span>
          </span>
        )}
        <span
          id="footer-note"
          role={footer.error ? "status" : undefined}
          className={footer.error ? "text-red" : "text-muted"}
        >
          {footer.note}
        </span>
      </span>
      <div className="flex shrink-0 items-center gap-2 phone:flex-wrap phone:justify-end phone:[&>button]:flex-auto">
        {footer.actions.map((action) => (
          <ActionButton key={action.label} view={action} describedBy="footer-note" />
        ))}
      </div>
    </footer>
  );
}

/** The footer of the open action, whatever its kind. */
export const footerOf = (view: ActionView): FooterView =>
  view.kind === "tactical" ? view.task.footer : view.footer;

interface TaskPanelProps {
  heading: string;
  interrupt: InterruptView | null;
  pill: PillView | null;
  trail: TrailItem[];
  edit?: TaskView["edit"];
  /** Rules and explanations. They are shown on hover or click only. */
  help: HelpView[];
  /** Null when the shell shows the footer itself. */
  footer: FooterView | null;
  /** Scroll position is kept while this stays the same. */
  contentKey: string;
  labelledBy?: string;
  reveal: string | null;
  children: ReactNode;
}

/** The open step: heading with its state, the content, and a footer that says what the main button does. */
function TaskPanel({
  heading,
  interrupt,
  pill,
  trail,
  edit,
  help,
  footer,
  contentKey,
  labelledBy,
  reveal,
  children,
}: TaskPanelProps) {
  const dispatch = useDispatch();
  const content = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (reveal) {
      content.current
        ?.querySelector(`[data-link~="${reveal}"]`)
        ?.scrollIntoView({ block: "nearest" });
    }
  }, [reveal]);
  return (
    <section
      id="step-panel"
      role="tabpanel"
      aria-labelledby={labelledBy}
      tabIndex={-1}
      className="flex min-h-0 min-w-0 flex-1 flex-col outline-none"
    >
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 border-b border-line px-5 py-[9px] phone:px-3">
        <h2 className="text-lg font-semibold tracking-[-.2px]">{heading}</h2>
        {pill && <Badge tone={pill.tone}>{pill.label}</Badge>}
        {help.length > 0 && (
          <Hint label={`Rules: ${heading}`}>
            {help.map((item) => (
              <p key={item.title}>
                <strong className="block">{item.title}</strong>
                {item.text}
              </p>
            ))}
          </Hint>
        )}
        <Trail items={trail} />
        {edit && (
          <Button
            tone="quiet"
            size="sm"
            icon="edit"
            className="ml-auto"
            disabled={edit.disabled}
            title={edit.hint}
            onClick={() => dispatch({ type: "editStep", step: edit.step })}
          >
            {edit.label}
          </Button>
        )}
      </div>
      {interrupt && <Interrupt view={interrupt} />}
      <div
        key={contentKey}
        ref={content}
        className="min-h-0 flex-1 space-y-2.5 overflow-y-auto px-5 pt-3 pb-4 phone:px-3"
      >
        {children}
      </div>
      {footer && <ActionFooter view={footer} />}
    </section>
  );
}

/**
 * The action panel: one frame for every action. A tactical action has five steps, a strategic action
 * has two, a component action has none. Live, Draft and History use the same frame.
 * The panel has one width for every step.
 */
export function ActionPanel({
  view,
  reveal,
  footer = true,
}: {
  view: ActionView;
  reveal: string | null;
  /** False when the shell shows `ActionFooter` itself, under every pane. */
  footer?: boolean;
}) {
  const dispatch = useDispatch();
  const away = view.kind === "tactical" && view.current !== null && view.selected !== view.current;
  const uncommitted = view.kind === "tactical" ? view.uncommitted : null;
  return (
    <aside className="flex min-h-0 min-w-0 flex-1 flex-col">
      {view.past && (
        <div className="flex items-center justify-between gap-2.5 border-b border-green/25 bg-green/[.07] py-1.5 pr-2.5 pl-4 text-sm">
          <span>
            <strong>History</strong> · {view.past.label} · read-only
          </span>
          <Button size="sm" iconAfter="arrow" onClick={() => dispatch({ type: "closeHistory" })}>
            Back to Live
          </Button>
        </div>
      )}
      <div className="flex min-h-10 items-center gap-2 border-b border-line bg-surface py-1 pr-2 pl-4">
        <strong className="whitespace-nowrap">{view.title}</strong>
        <span className="min-w-0 truncate text-sm text-muted">{view.subtitle}</span>
        <span className="ml-auto" />
        {view.badge && <Badge tone={view.badge.tone}>{view.badge.label}</Badge>}
      </div>
      {view.tabs.length > 0 && (
        <StepTabs
          label={`${view.title} steps`}
          tabs={view.tabs}
          selected={view.selected}
          current={view.current}
          onSelect={(step) => dispatch({ type: "selectStep", step })}
        />
      )}
      {(uncommitted || away) && (
        <div className="flex flex-none items-center justify-end gap-1.5 border-b border-line px-3 py-[5px]">
          {uncommitted && (
            <Button
              tone="quiet"
              size="sm"
              icon="edit"
              onClick={() => dispatch({ type: "returnToEdit" })}
            >
              {uncommitted.name} · uncommitted
            </Button>
          )}
          {away && (
            <Button
              tone="quiet"
              size="sm"
              iconAfter="arrow"
              onClick={() => dispatch({ type: "returnToCurrent" })}
            >
              Go to current
            </Button>
          )}
        </div>
      )}
      {view.kind === "tactical" ? (
        <TaskPanel
          heading={view.task.title}
          interrupt={view.interrupt}
          pill={view.task.pill}
          trail={view.task.trail}
          edit={view.task.edit}
          help={view.task.help}
          footer={footer ? view.task.footer : null}
          contentKey={view.contentKey}
          labelledBy={`step-tab-${view.selected}`}
          reveal={reveal}
        >
          <StepContent view={view.task.content} />
          <Blocks blocks={view.closing} />
        </TaskPanel>
      ) : (
        <TaskPanel
          heading={view.heading}
          interrupt={view.interrupt}
          pill={view.pill}
          trail={[]}
          help={view.help}
          footer={footer ? view.footer : null}
          contentKey={view.contentKey}
          reveal={reveal}
        >
          <Blocks blocks={view.blocks} />
          <Blocks blocks={view.closing} />
        </TaskPanel>
      )}
    </aside>
  );
}
