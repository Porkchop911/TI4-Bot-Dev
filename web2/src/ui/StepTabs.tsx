import { useEffect, useRef, type KeyboardEvent } from "react";
import { cx } from "./cx";
import { Icon } from "./Icon";

export type StepStatus =
  | "future"
  | "skipped"
  | "done"
  | "decision"
  | "needs-review"
  | "boundary"
  | "battle"
  | "draft";

export interface StepTab {
  name: string;
  status: StepStatus;
  caption: string;
}

export interface StepTabsProps {
  label: string;
  tabs: StepTab[];
  selected: number;
  /** The step the action is waiting at; null when it is complete. */
  current: number | null;
  onSelect: (index: number) => void;
  panelId?: string;
}

/** The steps of one action. A step that is not reached is locked. */
export function StepTabs({
  label,
  tabs,
  selected,
  current,
  onSelect,
  panelId = "step-panel",
}: StepTabsProps) {
  const list = useRef<HTMLDivElement>(null);
  // Where the row of steps scrolls (a phone), the selected step is in view.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the selected step moves the row
  useEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selected]);
  const reached = tabs
    .map((tab, index) => (tab.status === "future" ? -1 : index))
    .filter((index) => index >= 0);

  function onKeyDown(event: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key) || !reached.length) {
      return;
    }
    event.preventDefault();
    const at = reached.indexOf(selected);
    const next =
      event.key === "Home"
        ? reached[0]
        : event.key === "End"
          ? reached[reached.length - 1]
          : reached[(at + (event.key === "ArrowRight" ? 1 : -1) + reached.length) % reached.length];
    onSelect(next);
    requestAnimationFrame(() =>
      list.current?.querySelector<HTMLElement>(`#step-tab-${next}`)?.focus({ preventScroll: true }),
    );
  }

  return (
    <div className="flex min-h-10 items-stretch gap-2 border-b border-line bg-surface px-1.5">
      <div
        ref={list}
        role="tablist"
        aria-label={label}
        onKeyDown={onKeyDown}
        className="flex min-w-0 flex-1 overflow-x-auto [scrollbar-width:none]"
      >
        {tabs.map((tab, index) => {
          const isCurrent = current === index;
          const review = tab.status === "needs-review";
          const chip = review
            ? "border-red bg-red font-bold text-canvas"
            : isCurrent
              ? "border-accent bg-accent font-bold text-canvas"
              : tab.status === "done" || tab.status === "skipped"
                ? "border-green/20 bg-green/[.035] text-green"
                : tab.status === "draft"
                  ? "border-dashed border-gold text-gold"
                  : "border-[#46546a] text-muted";
          return (
            <button
              key={tab.name}
              type="button"
              role="tab"
              id={`step-tab-${index}`}
              aria-controls={panelId}
              aria-selected={selected === index}
              aria-current={isCurrent ? "step" : undefined}
              tabIndex={selected === index ? 0 : -1}
              disabled={tab.status === "future"}
              title={`${tab.name}${tab.caption ? " · " + tab.caption : ""}`}
              data-state={tab.status}
              onClick={() => onSelect(index)}
              // On a phone the row of steps scrolls sideways: a step keeps its name.
              className="flex min-w-0 flex-[1_1_0] items-center gap-[7px] border-b-[3px] border-transparent px-2.5 text-left text-sm whitespace-nowrap enabled:hover:bg-white/[.02] disabled:text-faint aria-selected:border-b-cyan aria-selected:bg-cyan/[.04] phone:flex-none phone:px-3"
            >
              <span
                className={cx(
                  "grid size-[22px] shrink-0 place-items-center rounded-full border text-xs",
                  chip,
                )}
              >
                {tab.status === "future" ? (
                  <Icon name="lock" className="size-3" />
                ) : tab.status === "skipped" ? (
                  <Icon name="minus" className="size-3" />
                ) : tab.status === "done" ? (
                  <Icon name="check" className="size-3" />
                ) : (
                  index + 1
                )}
              </span>
              <span className="font-strong">{tab.name}</span>
              <span
                className={cx(
                  "min-w-0 overflow-hidden text-xs text-ellipsis phone:hidden",
                  review ? "text-red" : isCurrent ? "text-accent" : "text-muted",
                )}
              >
                {tab.caption}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
