import { payState } from "../steps/payState";
import type { ReactNode } from "react";
import type {
  ActionView,
  FooterView,
  HelpView,
  InterruptView,
  PillView,
  ReviewView,
  StepContentView,
  TaskView,
} from "../../model";
import { informs } from "../../model";
import { Badge, Button, Card, Hint, ListRow, StepTabs, Trail, cx, type TrailItem } from "../../ui";
import { ActionButton, useDispatch } from "../context";
import { Blocks } from "../flows/Blocks";
import { useLink } from "../link";
import { ActivationStep } from "../steps/ActivationStep";
import { InvasionStep, SpaceCombatStep } from "../steps/BattleSteps";
import { MovementStep } from "../steps/MovementStep";
import { ProductionStep } from "../steps/ProductionStep";

/** The content of a step of a tactical action. */
export function StepContent({ view }: { view: StepContentView }) {
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

/**
 * A window on top of the open action: a reaction window with the cards that can be played now, or
 * an ability that asks in the middle of the action, its card in one line with the text behind "?".
 * What opened it is its name; the heading and its context say it on the screen.
 */
export function Interrupt({ view, className }: { view: InterruptView; className?: string }) {
  const { linked, props } = useLink(view.link);
  return (
    <div
      role="group"
      aria-label={view.label}
      className={cx(
        "space-y-1.5 border-b border-cyan/30 bg-cyan/[.05] px-5 py-2 phone:px-3",
        linked && "bg-cyan/[.09]",
        className,
      )}
      {...props}
    >
      {view.source && (
        <p className="flex min-w-0 flex-wrap items-baseline gap-x-2">
          <strong className="text-base font-semibold">{view.source.name}</strong>
          <span className="text-xs text-muted">{view.source.type}</span>
          {view.source.text && (
            <span className="self-center">
              <Hint label={`Text: ${view.source.name}`}>{view.source.text}</Hint>
            </span>
          )}
        </p>
      )}
      {view.question && <p className="text-sm">{view.question}</p>}
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

/**
 * Minimizes the step sheet ("▾", open) or restores it ("▴", minimized): the sheet goes down to its
 * heading line, and the board is in view.
 */
export function SheetToggle({ open, onToggle }: { open: boolean; onToggle?: () => void }) {
  const label = open ? "Minimize" : "Restore";
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-expanded={open}
      onClick={onToggle}
      className="-mx-1 shrink-0 self-center rounded px-1 text-muted hover:text-text touch:min-h-9 touch:min-w-9"
    >
      <span aria-hidden="true">{open ? "▾" : "▴"}</span>
    </button>
  );
}

/**
 * The pill of the heading, less what the toolbar already says: "Your decision" is gone, and
 * "Your decision · Reaction" is "Reaction". A pill that says something else stays as it is.
 */
export function pillBeyond(pill: PillView | null, status: string): PillView | null {
  if (!pill || pill.label === status) {
    return null;
  }
  const prefix = `${status} · `;
  if (!pill.label.startsWith(prefix)) {
    return pill;
  }
  const rest = pill.label.slice(prefix.length);
  return { ...pill, label: rest.charAt(0).toUpperCase() + rest.slice(1) };
}

/** The footer of the open action, whatever its kind. */
export const footerOf = (view: ActionView): FooterView =>
  view.kind === "tactical" ? view.task.footer : view.footer;

interface HeadingRowProps {
  /** Null: the selected step tab names it (a tactical action), and the context leads the row. */
  heading: string | null;
  /** What the heading does not say, muted beside it. */
  context: string | null;
  pill: PillView | null;
  /** "Not sent" while a choice is staged. */
  badge: PillView | null;
  trail: TrailItem[];
  edit?: TaskView["edit"];
  /** Rules and explanations. They are shown on hover or click only. */
  help: HelpView[];
  /** Minimizes the step sheet to its heading line. Absent where the panel is not a sheet. */
  onMinimize?: () => void;
}

/**
 * The first row of the panel: the heading, or for a tactical action its context (the step is the
 * selected tab under it), with its state and the controls of the sheet.
 */
function HeadingRow({
  heading,
  context,
  pill,
  badge,
  trail,
  edit,
  help,
  onMinimize,
}: HeadingRowProps) {
  const dispatch = useDispatch();
  const lead = heading ?? context ?? "";
  return (
    <div className="flex flex-none flex-wrap items-center gap-x-2.5 gap-y-1.5 border-b border-line px-5 py-[9px] phone:px-3">
      {/* One line: the context gives way with "…" before the pill, "Not sent" or "?" wrap. */}
      {/* The context of a tactical action gives way with "…": its pill stays on the line. */}
      <span
        className={cx(
          "flex min-w-0 max-w-full items-center gap-x-2.5 gap-y-1.5",
          heading && "phone:flex-wrap",
        )}
      >
        {onMinimize && <SheetToggle open onToggle={onMinimize} />}
        <h2
          className={cx(
            "min-w-0 truncate font-semibold tracking-[-.2px]",
            heading ? "text-lg phone:whitespace-normal" : "text-base",
          )}
          title={lead}
        >
          {lead}
        </h2>
        {heading && context && (
          <span className="min-w-0 shrink-[2] truncate text-sm text-muted" title={context}>
            {context}
          </span>
        )}
        {pill && (
          <Badge tone={pill.tone} className="shrink-0">
            {pill.label}
          </Badge>
        )}
        {badge && (
          <Badge tone={badge.tone} className="shrink-0">
            {badge.label}
          </Badge>
        )}
        {help.length > 0 && (
          <span className="shrink-0">
            <Hint label={`Rules: ${lead}`}>
              {help.map((item) => (
                <p key={item.title}>
                  <strong className="block">{item.title}</strong>
                  {item.text}
                </p>
              ))}
            </Hint>
          </span>
        )}
      </span>
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
  );
}

/** The open step: the band of a window, the content, and a footer that says what the main button does. */
function TaskPanel({
  interrupt,
  footer,
  contentKey,
  labelledBy,
  children,
}: {
  interrupt: InterruptView | null;
  /** Null when the shell shows the footer itself. */
  footer: FooterView | null;
  /** Scroll position is kept while this stays the same. */
  contentKey: string;
  labelledBy?: string;
  children: ReactNode;
}) {
  return (
    <section
      id="step-panel"
      role="tabpanel"
      aria-labelledby={labelledBy}
      tabIndex={-1}
      className="flex min-h-0 min-w-0 flex-1 flex-col outline-none"
    >
      {interrupt && <Interrupt view={interrupt} />}
      <div
        key={contentKey}
        className="min-h-0 flex-1 space-y-2.5 overflow-y-auto px-5 pt-3 pb-4 phone:px-3"
      >
        {children}
      </div>
      {footer && <ActionFooter view={footer} />}
    </section>
  );
}

/** The heading of a tactical action: none where the selected step tab says it already. */
export const tacticalHeading = (view: Extract<ActionView, { kind: "tactical" }>): string | null =>
  view.tabs.length > 0 && view.tabs[view.selected]?.name === view.task.title
    ? null
    : view.task.title;

/**
 * The action panel: one frame for every action. A tactical action has five steps, a strategic action
 * has two, a component action has none. Live, Draft and an opened turn of the Log use the same frame:
 * first the heading row with the context and the controls, then the step tabs of a tactical action,
 * then the step. The panel has one width for every step. It is the viewer's decision (the step
 * sheet): a block or a step that only informs (`BlockRole`) is not in it, but in the Log.
 */
export function ActionPanel({
  view,
  status,
  footer = true,
  onMinimize,
}: {
  view: ActionView;
  /** The status in the toolbar. The pill of the heading does not say it again. */
  status: string;
  /** False when the shell shows `ActionFooter` itself, under every pane. */
  footer?: boolean;
  /** Minimizes the sheet to its heading line: "▾" before the heading. */
  onMinimize?: () => void;
}) {
  const dispatch = useDispatch();
  const away = view.kind === "tactical" && view.current !== null && view.selected !== view.current;
  return (
    <aside className="flex min-h-0 min-w-0 flex-1 flex-col">
      {view.kind === "tactical" ? (
        <HeadingRow
          heading={tacticalHeading(view)}
          context={view.context}
          pill={pillBeyond(view.task.pill, status)}
          badge={view.badge}
          trail={view.task.trail}
          edit={view.task.edit}
          help={view.task.help}
          onMinimize={onMinimize}
        />
      ) : (
        <HeadingRow
          heading={view.heading}
          context={view.context}
          pill={pillBeyond(view.pill, status)}
          badge={view.badge}
          trail={view.trail}
          help={view.help}
          onMinimize={onMinimize}
        />
      )}
      {view.tabs.length > 0 && (
        <StepTabs
          label="Tactical action steps"
          tabs={view.tabs}
          selected={view.selected}
          current={view.current}
          onSelect={(step) => dispatch({ type: "selectStep", step })}
        />
      )}
      {away && (
        <div className="flex flex-none items-center justify-end gap-1.5 border-b border-line px-3 py-[5px]">
          <Button
            tone="quiet"
            size="sm"
            iconAfter="arrow"
            onClick={() => dispatch({ type: "returnToCurrent" })}
          >
            Go to current
          </Button>
        </div>
      )}
      {view.kind === "tactical" ? (
        <TaskPanel
          interrupt={view.interrupt}
          footer={footer ? view.task.footer : null}
          contentKey={view.contentKey}
          labelledBy={`step-tab-${view.selected}`}
        >
          {view.task.review && <Review view={view.task.review} />}
          {/* A step that only informs (a combat that the viewer watches) is of the Log. */}
          {view.task.role === "decide" && <StepContent view={view.task.content} />}
          <Blocks blocks={view.closing} />
        </TaskPanel>
      ) : (
        <TaskPanel
          interrupt={view.interrupt}
          footer={footer ? view.footer : null}
          contentKey={view.contentKey}
        >
          {/* The blocks that only inform (the other seats, what happened) are of the Log. */}
          <Blocks blocks={view.blocks.filter((block) => !informs(block))} />
          <Blocks blocks={view.closing} />
        </TaskPanel>
      )}
    </aside>
  );
}

/** The entry of a draft that no longer fits: one marked row in its step, with what removes it. */
export function Review({ view }: { view: ReviewView }) {
  return (
    <Card role="group" aria-label="Needs review">
      <ListRow
        invalid
        icon={view.icon ?? "alert"}
        // A sign, not only the colour: this entry no longer fits.
        title={`✗ ${view.title}`}
        subtitle={view.reason}
        controls={view.remove && <ActionButton view={view.remove} size="sm" />}
      />
    </Card>
  );
}
