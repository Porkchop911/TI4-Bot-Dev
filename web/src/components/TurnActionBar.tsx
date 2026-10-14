import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ValueText } from "./PlanetValueIcons.tsx";
import { humanizeId } from "../protocol/contentCatalog.ts";
import {
  type BarAction,
  type BarInfo,
  type BarItem,
  type StrategicButton,
  type TradePartnerView,
  type TurnBarModel,
} from "../presentation/turnBar.ts";
import { useBottomBarOffset } from "../hooks/useBottomBarOffset.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import "./TurnActionBar.css";

export interface TurnActionBarProps {
  model: TurnBarModel;
  /** Send the engine option a button stands for. */
  onSubmit: (optionId: string) => Promise<void>;
  lastError?: string | null;
}

type MenuKey = "components" | "cards" | "trade";

const MAX_PIPS = 12;

/** The height left above the bar for an open menu (set while a menu is open). */
export const MENU_ROOM_VAR = "--turn-bar-menu-room";
/** Phone sheet edges while a menu is open: just below the game header, and the bar's bottom edge. */
export const MENU_TOP_VAR = "--turn-bar-menu-top";
export const MENU_BOTTOM_VAR = "--turn-bar-menu-bottom";

/** The viewer's single preference for the phone layout: is the action pane minimized to a slim bar. */
export const MINIMIZED_KEY = "ti4_action_pane_minimized";

function readMinimized(): boolean {
  try {
    return window.localStorage.getItem(MINIMIZED_KEY) === "1";
  } catch {
    return false;
  }
}

function writeMinimized(value: boolean): void {
  try {
    window.localStorage.setItem(MINIMIZED_KEY, value ? "1" : "0");
  } catch {
    /* storage blocked: the choice lasts until the page reloads */
  }
}

/**
 * Whether the bar was last shown read-only (another seat's turn). Module level so a remount between
 * the read-only and the active bar does not forget it. Rule: the pane expands once when it turns
 * from another seat's turn into the viewer's own, so the viewer notices; every other render, and
 * a pane the viewer minimized during this decision, is left as it is.
 */
let lastShownReadOnly = false;
export function resetActionPaneMemory(): void {
  lastShownReadOnly = false;
}

const Pips: React.FC<{ count: number }> = ({ count }) => (
  <span className="turn-bar__pips" aria-hidden="true">
    {Array.from({ length: Math.min(Math.max(count, 0), MAX_PIPS) }, (_, i) => (
      <i key={i} />
    ))}
  </span>
);

const KEY_HINTS: [string, string][] = [
  ["T", "tactical"],
  ["S", "strategic (several cards: S cycles, Enter picks)"],
  ["C", "components"],
  ["A", "action cards"],
  ["D", "trade (deal)"],
  ["P", "pass"],
  ["Enter", "end turn when no button has focus"],
  ["Esc", "close menu"],
];

function typingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || !el.tagName) return false;
  return (
    el.tagName === "INPUT" ||
    el.tagName === "TEXTAREA" ||
    el.tagName === "SELECT" ||
    el.isContentEditable
  );
}

interface BarButtonProps {
  barKey: string;
  testId: string;
  title: string;
  hint: string;
  reason: string | null;
  enabled: boolean;
  busy: boolean;
  kbd?: string;
  count?: number;
  primary?: boolean;
  glow?: boolean;
  ghost?: boolean;
  expanded?: boolean;
  infoValue?: BarInfo | null;
  onActivate: () => void;
  onInfo: (info: BarInfo | null) => void;
  extra?: string;
}

const BarButton: React.FC<BarButtonProps> = (p) => (
  <button
    type="button"
    className={[
      "turn-bar__btn",
      p.primary && "turn-bar__btn--primary",
      p.glow && "turn-bar__btn--glow",
      p.ghost && "turn-bar__btn--ghost",
      p.extra,
    ]
      .filter(Boolean)
      .join(" ")}
    data-testid={p.testId}
    data-bar-key={p.barKey}
    aria-disabled={!p.enabled || p.busy}
    aria-expanded={p.expanded === undefined ? undefined : p.expanded}
    aria-haspopup={p.expanded === undefined ? undefined : "true"}
    aria-keyshortcuts={p.kbd}
    title={p.enabled ? undefined : (p.reason ?? undefined)}
    onClick={() => {
      if (p.enabled && !p.busy) p.onActivate();
    }}
    onMouseEnter={() => p.infoValue && p.onInfo(p.infoValue)}
    onFocus={() => p.infoValue && p.onInfo(p.infoValue)}
    onMouseLeave={() => p.onInfo(null)}
    onBlur={() => p.onInfo(null)}
  >
    <span className="turn-bar__title">
      <span>{p.title}</span>
      {p.count !== undefined ? (
        <span className="turn-bar__count">{p.count}</span>
      ) : p.kbd ? (
        <kbd>{p.kbd}</kbd>
      ) : null}
    </span>
    <span className="turn-bar__sub">{p.enabled ? p.hint : (p.reason ?? p.hint)}</span>
  </button>
);

/**
 * The persistent action bar for the action phase. One bar that never moves: Tactical, one
 * Strategic button per held strategy card, Components, Action cards, Trade, Pass and End turn.
 * Disabled buttons stay in place and say why; card text appears in a side panel on hover or focus.
 * Letter keys trigger their button: a single option submits, a picker opens, several strategy cards
 * take focus and cycle. Enter ends the turn when no button has focus.
 */
export const TurnActionBar: React.FC<TurnActionBarProps> = ({ model, onSubmit, lastError }) => {
  const display = usePlayerIdentity();
  const rootRef = useRef<HTMLElement | null>(null);
  useBottomBarOffset(rootRef);
  const [open, setOpen] = useState<MenuKey | null>(null);
  const [minimized, setMinimized] = useState<boolean>(readMinimized);
  const bodyId = React.useId();
  const [info, setInfo] = useState<BarInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const readOnly = model.mode === "readonly";

  // Minimizing only changes how the phone layout looks (CSS); the buttons stay mounted so the keys work.
  const setPaneMinimized = useCallback((value: boolean) => {
    setMinimized(value);
    writeMinimized(value);
  }, []);
  useEffect(() => {
    if (readOnly) {
      lastShownReadOnly = true;
      return;
    }
    if (lastShownReadOnly) {
      lastShownReadOnly = false;
      setPaneMinimized(false);
    }
  }, [readOnly, setPaneMinimized]);

  const submit = useCallback(
    async (optionId: string | null) => {
      if (!optionId || busy || readOnly) return;
      setBusy(true);
      setError(null);
      setOpen(null);
      setInfo(null);
      try {
        await onSubmit(optionId);
      } catch (err: unknown) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setBusy(false);
      }
    },
    [busy, onSubmit, readOnly],
  );

  const [notice, setNotice] = useState<string | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const flash = (text: string) => {
    setNotice(text);
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(null), 2500);
  };
  useEffect(() => () => {
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
  }, []);

  const toggleMenu = (key: MenuKey) => setOpen((current) => (current === key ? null : key));

  useEffect(() => {
    if (readOnly) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.metaKey || event.altKey || event.defaultPrevented || event.repeat) return;
      if (typingTarget(event.target) || typingTarget(document.activeElement)) return;
      // A dialog owns the keyboard; the bar must not act behind it.
      if (document.querySelector('[role="dialog"][aria-modal="true"]')) return;
      const focused = document.activeElement as HTMLElement | null;
      const foreign = focused?.closest?.('[role="dialog"],[role="alertdialog"]');
      if (foreign && !rootRef.current?.contains(foreign)) return;
      const key = event.key;
      if (key === "Escape") {
        if (open) {
          setOpen(null);
          setInfo(null);
          event.preventDefault();
        }
        return;
      }
      if (key === "Enter") {
        const active = document.activeElement as HTMLElement | null;
        const onControl = active && (active.tagName === "BUTTON" || active.tagName === "A");
        if (!onControl && model.end.enabled) {
          event.preventDefault();
          void submit(model.end.optionId);
        }
        return;
      }
      const lower = key.toLowerCase();
      const direct = (a: BarAction) => {
        if (a.enabled) void submit(a.optionId);
        else flash(a.reason ?? "Not available");
      };
      const picker = (a: { enabled: boolean; reason: string | null }, menu: MenuKey) => {
        if (a.enabled) setOpen(menu);
        else flash(a.reason ?? "Not available");
      };
      const strategic = () => {
        const cards = model.strategic;
        if (cards.length === 1) {
          if (cards[0].enabled) void submit(cards[0].optionId);
          else flash(cards[0].reason ?? "Not available");
          return;
        }
        const nodes = Array.from(
          rootRef.current?.querySelectorAll<HTMLElement>('[data-bar-key="strategic"]') ?? [],
        );
        const usable = nodes.filter((n) => n.getAttribute("aria-disabled") !== "true");
        if (usable.length === 0) {
          flash(cards[0]?.reason ?? "No strategy card");
          return;
        }
        const at = usable.indexOf(document.activeElement as HTMLElement);
        usable[(at + 1) % usable.length].focus();
      };
      const actions: Record<string, () => void> = {
        t: () => direct(model.tactical),
        p: () => direct(model.pass),
        s: strategic,
        c: () => picker(model.components, "components"),
        a: () => picker(model.actionCards, "cards"),
        d: () => picker(model.trade, "trade"),
      };
      const action = actions[lower];
      if (!action) return;
      event.preventDefault();
      action();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [model, open, readOnly, submit]);

  // A menu opens upward from the bar: never taller than the room between the bar and the game header,
  // or its top (the first partner, the first components) ends up off screen on a phone.
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!open || !root) return;
    const place = () => {
      const header = document.querySelector(".app-shell__header");
      const top = Math.max(0, Math.ceil(header?.getBoundingClientRect().bottom ?? 0));
      const bar = root.getBoundingClientRect();
      root.style.setProperty(MENU_ROOM_VAR, `${Math.max(120, Math.floor(bar.top - top - 6 - 8))}px`);
      // Phone sheet: from below the header down to the bar's bottom edge (over the bar's buttons).
      root.style.setProperty(MENU_TOP_VAR, `${top + 6}px`);
      root.style.setProperty(
        MENU_BOTTOM_VAR,
        `${Math.max(0, Math.floor(window.innerHeight - bar.bottom))}px`,
      );
    };
    place();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    observer?.observe(root);
    window.addEventListener("resize", place);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", place);
      root.style.removeProperty(MENU_ROOM_VAR);
      root.style.removeProperty(MENU_TOP_VAR);
      root.style.removeProperty(MENU_BOTTOM_VAR);
    };
  }, [open]);

  // The first row of a freshly opened menu takes focus so the arrows work at once.
  useEffect(() => {
    if (!open) return;
    const first = rootRef.current?.querySelector<HTMLElement>(
      `[data-bar-menu="${open}"] [data-bar-row]:not([aria-disabled="true"])`,
    );
    first?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(null);
        setInfo(null);
      }
    };
    document.addEventListener("mousedown", onPointer);
    return () => document.removeEventListener("mousedown", onPointer);
  }, [open]);

  const onMenuKey = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>('[data-bar-row]:not([aria-disabled="true"])'),
    );
    if (rows.length === 0) return;
    const at = rows.indexOf(document.activeElement as HTMLElement);
    const next =
      event.key === "ArrowDown"
        ? (at + 1) % rows.length
        : at <= 0
          ? rows.length - 1
          : at - 1;
    rows[next].focus();
    event.preventDefault();
  };

  const infoHandlers = (value: BarInfo | null) => ({
    onMouseEnter: () => value && setInfo(value),
    onFocus: () => value && setInfo(value),
    onMouseLeave: () => setInfo(null),
    onBlur: () => setInfo(null),
  });

  const heading =
    model.mode === "readonly" && model.waitingFor
      ? `Waiting for ${display(model.waitingFor).label}`
      : model.heading;

  const actionBtn = (a: BarAction, kbd: string, extra?: Partial<BarButtonProps>) => (
    <BarButton
  busy={busy}
  onInfo={setInfo}
      barKey={a.key}
      testId={`turn-bar-${a.key}`}
      title={a.label}
      hint={a.hint}
      reason={a.reason}
      enabled={a.enabled}
      kbd={kbd}
      infoValue={a.info}
      onActivate={() => void submit(a.optionId)}
      {...extra}
    />
  );

  const strategicBtn = (s: StrategicButton, index: number) => (
    <BarButton
  busy={busy}
  onInfo={setInfo}
      key={`${s.cardId}-${index}`}
      barKey="strategic"
      testId={`turn-bar-strategic-${s.cardId || index}`}
      title={s.label}
      hint={s.cardName === s.label ? s.effect : `${s.cardName}: ${s.effect}`}
      reason={s.reason}
      enabled={s.enabled}
      kbd={index === 0 ? "S" : undefined}
      infoValue={s.info}
      extra={s.used ? "turn-bar__btn--used" : undefined}
      onActivate={() => void submit(s.optionId)}
    />
  );

  const row = (item: BarItem) => (
    <button
      key={item.optionId}
      type="button"
      role="menuitem"
      className="turn-bar__row"
      data-bar-row
      data-testid={`turn-bar-item-${item.optionId}`}
      onClick={() => void submit(item.optionId)}
      {...infoHandlers(item.info)}
    >
      <span className="turn-bar__row-label">{item.label}</span>
      {item.facts && item.facts.length > 0 && (
        <span className="turn-bar__row-facts" data-testid="turn-bar-row-facts">
          {item.facts.map((fact) => (
            <span key={fact} className="turn-bar__row-fact">
              {fact}
            </span>
          ))}
        </span>
      )}
      {item.detail && (
        <span
          className={item.facts ? "turn-bar__row-detail turn-bar__row-detail--full" : "turn-bar__row-detail"}
          data-testid="turn-bar-row-text"
        >
          {item.detail}
        </span>
      )}
      {item.note && <span className="turn-bar__row-note">{item.note}</span>}
    </button>
  );

  /** Title and Close for the phone sheet (hidden on wider screens, where the menu is a popover). */
  const menuHead = (title: string) => (
    <div className="turn-bar__menu-head">
      <span>{title}</span>
      <button
        type="button"
        className="turn-bar__menu-close"
        data-testid="turn-bar-menu-close"
        onClick={() => {
          setOpen(null);
          setInfo(null);
        }}
      >
        Close
      </button>
    </div>
  );

  const partnerName = (p: TradePartnerView) => {
    const seat = p.seat ? display(p.seat) : null;
    return { name: humanizeId(p.faction), color: seat?.color ?? "#94a3b8", who: seat?.label ?? null };
  };

  const holdings = (p: TradePartnerView): string[] => {
    const out: string[] = [];
    if (p.commodities !== null) out.push(`${p.commodities} commodities`);
    if (p.tradeGoods !== null) out.push(`${p.tradeGoods} trade goods`);
    if (p.promissoryNotes !== null)
      out.push(`${p.promissoryNotes} promissory note${p.promissoryNotes === 1 ? "" : "s"}`);
    return out;
  };

  const tokens = model.tokens;
  const errorText = error ?? lastError ?? null;
  // A rejected action must be seen: the error lives in the full pane.
  useEffect(() => {
    if (errorText) setMinimized(false);
  }, [errorText]);
  // A menu opened by key press needs the full pane too.
  useEffect(() => {
    if (open) setMinimized(false);
  }, [open]);
  const miniText = readOnly ? heading : `Your turn \u2014 ${heading.charAt(0).toLowerCase()}${heading.slice(1)}`;
  const tradeSub =
    model.trade.partners.length > 0
      ? `${model.trade.partners.length} partners · ${model.trade.available} in contact`
      : "";
  const cardWord = model.strategic.length > 1 ? "strategic cards" : "strategic card";

  return (
    <section
      ref={rootRef}
      className="turn-bar"
      data-testid="turn-action-bar"
      data-mode={model.mode}
      data-minimized={minimized ? "true" : "false"}
      data-menu-open={open ?? undefined}
      aria-label="Turn actions"
    >
      {info && (
        <aside className="turn-bar__info" data-testid="turn-bar-info" aria-live="polite">
          <div className="turn-bar__info-kind">{info.kind}</div>
          <h3>{info.title}</h3>
          {info.text && <p>{info.text}</p>}
          {info.secondary && <p className="turn-bar__info-secondary">{info.secondary}</p>}
          {info.planets && (
            <div className="turn-bar__planets" data-testid="turn-bar-info-planets">
              <div className="turn-bar__planets-title">
                Cultural planets you control (readied by this card)
              </div>
              {info.planets.length === 0 ? (
                <div className="turn-bar__planets-empty">None: the card would do nothing</div>
              ) : (
                <ul>
                  {info.planets.map((planet) => (
                    <li
                      key={planet.id}
                      data-testid={`turn-bar-info-planet-${planet.id}`}
                      data-exhausted={planet.exhausted}
                    >
                      <span>{planet.name}</span>
                      <span className="turn-bar__planet-trait">{planet.trait}</span>
                      <span
                        className={
                          planet.exhausted
                            ? "turn-bar__planet-state turn-bar__planet-state--changes"
                            : "turn-bar__planet-state"
                        }
                      >
                        {planet.exhausted ? "exhausted, will ready" : "already ready"}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
          {info.chips.length > 0 && (
            <div className="turn-bar__chips">
              {info.chips.map((chip) => (
                <span key={chip.label} className={`turn-bar__chip turn-bar__chip--${chip.tone}`}>
                  <ValueText text={chip.label} />
                </span>
              ))}
            </div>
          )}
        </aside>
      )}

      <div className="turn-bar__head">
        <strong data-testid="turn-bar-heading">{heading}</strong>
        <span className="turn-bar__mini-text" data-testid="turn-bar-mini-text">
          {miniText}
        </span>
        <button
          type="button"
          className="turn-bar__minimize"
          data-testid="turn-bar-minimize"
          aria-expanded={!minimized}
          aria-controls={bodyId}
          aria-label={minimized ? "Show actions" : "Minimize actions"}
          title={minimized ? "Show actions" : "Minimize actions"}
          onClick={() => setPaneMinimized(!minimized)}
        >
          <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false">
            <path d="M3 6l5 5 5-5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
        {tokens && (
          <span className="turn-bar__pools" data-testid="turn-bar-pools">
            <span className="turn-bar__pool" data-testid="turn-bar-pool-tactic">
              Tactic <Pips count={tokens.tactic} /> <b>{tokens.tactic}</b>
            </span>
            <span className="turn-bar__pool" data-testid="turn-bar-pool-fleet">
              Fleet <Pips count={tokens.fleet} /> <b>{tokens.fleet}</b>
            </span>
            <span className="turn-bar__pool" data-testid="turn-bar-pool-strategy">
              Strategy <Pips count={tokens.strategy} /> <b>{tokens.strategy}</b>
            </span>
          </span>
        )}
      </div>

      <div id={bodyId} className="turn-bar__body">
      <div className="turn-bar__actions" role="group" aria-label={`Actions: ${cardWord}`}>
        {actionBtn(model.tactical, "T")}
        {model.strategic.length === 0 ? (
          <BarButton
  busy={busy}
  onInfo={setInfo}
            barKey="strategic"
            testId="turn-bar-strategic-none"
            title="Strategic action"
            hint=""
            reason={readOnly ? model.tactical.reason : "No strategy card"}
            enabled={false}
            onActivate={() => undefined}
          />
        ) : (
          model.strategic.map(strategicBtn)
        )}

        <div className="turn-bar__wrap">
          <BarButton
  busy={busy}
  onInfo={setInfo}
            barKey="components"
            testId="turn-bar-components"
            title="Components"
            hint={`${model.components.count} usable`}
            reason={model.components.reason}
            enabled={model.components.enabled}
            kbd="C"
            expanded={open === "components"}
            onActivate={() => toggleMenu("components")}
          />
          {open === "components" && (
            <div
              className="turn-bar__menu"
              role="menu"
              aria-label="Components"
              data-bar-menu="components"
              data-testid="turn-bar-menu-components"
              onKeyDown={onMenuKey}
            >
              {menuHead(`Components (${model.components.count})`)}
              {model.components.groups.map((group) => (
                <div key={group.title} role="group" aria-label={group.title}>
                  <h4>{group.title}</h4>
                  {group.items.map(row)}
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="turn-bar__wrap">
          <BarButton
  busy={busy}
  onInfo={setInfo}
            barKey="cards"
            testId="turn-bar-cards"
            title="Action cards"
            hint={`${model.actionCards.count} playable now`}
            reason={model.actionCards.reason}
            enabled={model.actionCards.enabled}
            kbd="A"
            count={model.actionCards.count}
            expanded={open === "cards"}
            onActivate={() => toggleMenu("cards")}
          />
          {open === "cards" && (
            <div
              className="turn-bar__menu"
              role="menu"
              aria-label="Action cards"
              data-bar-menu="cards"
              data-testid="turn-bar-menu-cards"
              onKeyDown={onMenuKey}
            >
              {menuHead(`Action cards (${model.actionCards.count})`)}
              <h4>Action cards with an Action window</h4>
              {model.actionCards.items.map(row)}
            </div>
          )}
        </div>

        <div className="turn-bar__wrap">
          <BarButton
  busy={busy}
  onInfo={setInfo}
            barKey="trade"
            testId="turn-bar-trade"
            title="Trade"
            hint={tradeSub}
            reason={model.trade.reason}
            enabled={model.trade.enabled}
            kbd="D"
            expanded={open === "trade"}
            onActivate={() => toggleMenu("trade")}
          />
          {open === "trade" && (
            <div
              className="turn-bar__menu turn-bar__menu--trade"
              role="dialog"
              aria-label="Open a transaction"
              data-bar-menu="trade"
              data-testid="turn-bar-menu-trade"
              onKeyDown={onMenuKey}
            >
              {menuHead("Trade")}
              <h4>Open a transaction with&hellip;</h4>
              {model.trade.partners.map((p) => {
                const who = partnerName(p);
                return (
                  <div
                    key={p.faction}
                    className="turn-bar__partner"
                    data-testid={`turn-bar-partner-${p.faction}`}
                    data-available={p.available}
                  >
                    <div className="turn-bar__partner-name">
                      <i style={{ background: who.color }} aria-hidden="true" />
                      <span>{who.name}</span>
                      {who.who && <span className="turn-bar__partner-who">{who.who}</span>}
                      <span
                        className={`turn-bar__chip turn-bar__chip--${p.available ? "ok" : "bad"}`}
                      >
                        {p.available
                          ? p.inContact === false
                            ? "Can trade"
                            : "In contact"
                          : (p.reason ?? "Not available")}
                      </span>
                    </div>
                    <button
                      type="button"
                      className="turn-bar__open"
                      data-bar-row
                      data-testid={`turn-bar-open-${p.faction}`}
                      aria-disabled={!p.available}
                      aria-label={`Open a transaction with ${who.name}`}
                      onClick={() => {
                        if (p.available) void submit(p.optionId);
                      }}
                    >
                      Open
                    </button>
                    <div className="turn-bar__partner-what">
                      {holdings(p).map((text) => (
                        <span key={text} className="turn-bar__chip">
                          {text}
                        </span>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <span className="turn-bar__spacer" />
        {actionBtn(model.pass, "P", { ghost: true, extra: "turn-bar__btn--pass" })}
        {actionBtn(model.end, "Enter", {
          primary: model.end.enabled,
          glow: model.end.enabled,
        })}
      </div>

      {errorText && (
        <div className="turn-bar__error" role="alert" data-testid="turn-bar-error">
          {errorText}
        </div>
      )}

      {notice && (
        <div className="turn-bar__notice" role="status" data-testid="turn-bar-notice">
          {notice}
        </div>
      )}

      </div>

      {!readOnly && (
        <div className="turn-bar__hints" aria-label="Keyboard shortcuts">
          {KEY_HINTS.map(([key, text]) => (
            <span key={key}>
              <kbd>{key}</kbd> {text}
            </span>
          ))}
        </div>
      )}
    </section>
  );
};
