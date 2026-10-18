import { useEffect, useReducer, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cx } from "./cx";
import { Button, CloseButton } from "./Button";
import type { IconName } from "./Icon";

export interface MenuItem {
  label: string;
  onSelect: () => void;
}

/** A small menu behind an icon button. */
export function Menu({
  label,
  icon = "more",
  items,
}: {
  label: string;
  icon?: IconName;
  items: MenuItem[];
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) {
      return;
    }
    const close = (event: Event) => {
      if (
        event instanceof KeyboardEvent
          ? event.key === "Escape"
          : !root.current?.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);
  return (
    <div ref={root} className="relative">
      <Button
        tone="quiet"
        size="icon"
        icon={icon}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      />
      {open && (
        <div
          role="menu"
          className="absolute top-[calc(100%+6px)] right-0 z-10 grid min-w-[170px] rounded-lg border border-[#415065] bg-raised p-[5px] shadow-pop"
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              className="rounded-[5px] px-2.5 py-2 text-left text-sm hover:bg-[#2a3a50] touch:py-3"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * A text that a trigger shows on hover or focus, and keeps open on a click or a tap. Esc and a
 * click outside close it. It opens below the trigger, or above it when there is no room below.
 * `Hint` is the "?" version; a name or a value can be the trigger too.
 */
export function Peek({
  label,
  trigger,
  buttonClassName,
  width: wanted = 340,
  flush = false,
  tall = false,
  children,
}: {
  label: string;
  trigger: ReactNode;
  /** The classes of the trigger button; `pinned` is true while the text is kept open. */
  buttonClassName: (pinned: boolean) => string;
  /** The width of the text in pixels, when the window has the room. */
  width?: number;
  /** The content sets its own padding: a card with sections. */
  flush?: boolean;
  /**
   * The text can be longer than the window is high (a whole sheet): it opens on the side of the
   * trigger with more room, and scrolls inside that room.
   */
  tall?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState<"peek" | "pinned" | null>(null);
  const root = useRef<HTMLSpanElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const note = useRef<HTMLDivElement>(null);
  const [, place] = useReducer((count: number) => count + 1, 0);
  useEffect(() => {
    if (!open) {
      return;
    }
    // Esc closes the text however it was opened. A click outside closes a pinned text.
    const close = (event: Event) => {
      if (
        event instanceof KeyboardEvent
          ? event.key === "Escape"
          : open === "pinned" &&
            !root.current?.contains(event.target as Node) &&
            !note.current?.contains(event.target as Node)
      ) {
        setOpen(null);
      }
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", close);
    // The text follows its button when a parent scrolls or the window changes.
    document.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", close);
      document.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [open]);
  const peek = () => setOpen((now) => now ?? "peek");
  const unpeek = () => setOpen((now) => (now === "peek" ? null : now));
  // Fixed position: the text is not cut by a scrolling parent.
  const rect = open ? button.current?.getBoundingClientRect() : undefined;
  const width = Math.min(wanted, window.innerWidth - 16);
  // Above the trigger when the room below is small and there is more of it above.
  const below = rect ? window.innerHeight - rect.bottom : 0;
  const above = !!rect && (tall || below < 200) && rect.top > below;
  const room = rect ? (above ? rect.top : below) - 14 : 0;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: hover only shows the text early; the button inside is the control
    <span
      ref={root}
      className="relative inline-flex min-w-0"
      onMouseEnter={peek}
      onMouseLeave={unpeek}
    >
      <button
        ref={button}
        type="button"
        aria-label={label}
        aria-expanded={!!open}
        onFocus={peek}
        onBlur={unpeek}
        onClick={(event) => {
          event.stopPropagation();
          setOpen(open === "pinned" ? null : "pinned");
        }}
        className={buttonClassName(open === "pinned")}
      >
        {trigger}
      </button>
      {rect &&
        // Out of the layout of the trigger: a row after it (its own "?") never paints over the
        // text. In an open dialog it stays in the dialog, which is above the page.
        createPortal(
          <div
            ref={note}
            role="note"
            onMouseEnter={peek}
            onMouseLeave={unpeek}
            className={cx("fixed z-30", above ? "pb-1.5" : "pt-1.5")}
            style={{
              ...(above ? { bottom: window.innerHeight - rect.top } : { top: rect.bottom }),
              left: Math.max(8, Math.min(rect.left - 12, window.innerWidth - width - 8)),
              width,
            }}
          >
            <div
              className={cx(
                "rounded-lg border border-[#415065] bg-raised text-xs leading-[1.45] font-normal whitespace-normal text-text shadow-pop",
                !flush && "space-y-2 px-3 py-2.5",
                tall && "overflow-y-auto overscroll-contain",
              )}
              style={tall ? { maxHeight: room } : undefined}
            >
              {children}
            </div>
          </div>,
          button.current?.closest("dialog") ?? document.body,
        )}
    </span>
  );
}

/**
 * Rule or help text behind a "?" button. Hover or focus shows it; a click keeps it open.
 * The players are experts, so this text is never in the layout.
 */
export function Hint({
  label,
  children,
  ...peek
}: {
  label: string;
  children: ReactNode;
} & Pick<Parameters<typeof Peek>[0], "width" | "tall">) {
  return (
    <Peek
      {...peek}
      label={label}
      trigger="?"
      buttonClassName={(pinned) =>
        cx(
          // Under a finger the button keeps its size in the layout; the area that takes the tap is larger.
          "relative grid size-[18px] shrink-0 place-items-center rounded-full border text-2xs leading-none font-bold hover:border-line-strong hover:text-text touch:size-6 touch:text-xs touch:after:absolute touch:after:-inset-2.5",
          pinned ? "border-cyan text-cyan" : "border-line text-faint",
        )
      }
    >
      {children}
    </Peek>
  );
}

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  /** Shown above the title, for example a badge. */
  lead?: ReactNode;
  description?: string;
  children?: ReactNode;
  actions: ReactNode;
}

export function Dialog({
  open,
  onClose,
  title,
  lead,
  description,
  children,
  actions,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) {
      return;
    }
    if (open && !dialog.open) {
      dialog.showModal();
    }
    if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);
  return (
    <dialog
      ref={ref}
      onClose={onClose}
      aria-labelledby="dialog-title"
      aria-describedby={description ? "dialog-description" : undefined}
      className="m-auto w-[min(470px,calc(100vw-32px))] rounded-xl border border-[#415065] bg-surface p-0 text-text shadow-[0_30px_100px_#0007] backdrop:bg-[#02050ac9] backdrop:backdrop-blur-[3px]"
    >
      {open && (
        <>
          <div className="p-[22px]">
            {lead}
            <h2 id="dialog-title" className="mt-2.5 text-[20px] font-bold">
              {title}
            </h2>
            {description && (
              <p id="dialog-description" className="mt-1.5 mb-3.5 text-sm text-muted">
                {description}
              </p>
            )}
            {children}
          </div>
          <div className="flex justify-end gap-[9px] border-t border-line px-[22px] py-3.5">
            {actions}
          </div>
        </>
      )}
    </dialog>
  );
}

/** A reference sheet that slides over the board. */
export function Drawer({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <aside
      aria-label={title}
      className="absolute inset-y-0 left-0 z-5 flex w-[min(420px,94%)] flex-col border-r border-line bg-surface shadow-[12px_0_30px_#0006] phone:w-full phone:border-r-0"
    >
      <div className="flex items-center justify-between border-b border-line py-2 pr-2.5 pl-4">
        <h2 className="text-md font-semibold">{title}</h2>
        <CloseButton label={`Close ${title}`} onClick={onClose} />
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-4 pt-1 pb-4 text-sm">{children}</div>
    </aside>
  );
}

/** Shows the latest message for a short time. A new `id` shows it again. */
export function Toast({ message }: { message: { id: number; text: string } | null }) {
  const [visible, setVisible] = useState(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new id shows the message again; the object may be new for every view
  useEffect(() => {
    if (!message) {
      return setVisible(false);
    }
    setVisible(true);
    const timer = setTimeout(() => setVisible(false), 3500);
    return () => clearTimeout(timer);
  }, [message?.id]);
  return (
    <div
      role="status"
      aria-live="polite"
      className={cx(
        "pointer-events-none fixed bottom-24 left-1/2 z-20 w-max max-w-[calc(100vw-32px)] -translate-x-1/2 rounded border border-[#4c6885] bg-[#223346] px-4 py-2.5 text-sm shadow-[0_10px_30px_#0005] transition-[opacity,translate] duration-200",
        visible ? "translate-y-0 opacity-100" : "translate-y-3 opacity-0",
      )}
    >
      {message?.text}
    </div>
  );
}
