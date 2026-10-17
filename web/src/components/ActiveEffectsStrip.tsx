import React, { useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ActiveEffectDto } from "../protocol/types.ts";
import { describeActiveEffect, type ActiveEffectText } from "../presentation/effectText.ts";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import "./ActiveEffectsStrip.css";

interface Row {
  key: string;
  effect: ActiveEffectDto;
  text: ActiveEffectText;
  mine: boolean;
}

/**
 * The lingering effects of played action cards that are in force now, one chip per effect with the
 * seat it belongs to. Tapping the line opens the full text of each effect and when it ends. Renders
 * nothing while there are none, so a game without such cards looks exactly as before. It publishes
 * its height on the document (`--effects-strip-height`, `data-effects-strip`) so the phone's
 * connection chip keeps clear.
 */
export const ActiveEffectsStrip: React.FC<{
  effects: readonly ActiveEffectDto[] | undefined;
  viewerSeat?: string | null;
}> = ({ effects, viewerSeat }) => {
  const display = usePlayerIdentity();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const seatLabel = (id: string) => (id === viewerSeat ? "You" : display(id).label);
  const rows = useMemo<Row[]>(() => {
    const labels = { seat: (id: string) => (id === viewerSeat ? "You" : display(id).label), viewerSeat };
    const out: Row[] = [];
    (effects ?? []).forEach((effect, index) => {
      const text = describeActiveEffect(effect, labels);
      if (text) {
        out.push({ key: `${effect.seat}:${effect.effect}:${index}`, effect, text, mine: effect.seat === viewerSeat });
      }
    });
    // The viewer's own effects first; the engine's table order otherwise.
    return out.sort((a, b) => Number(b.mine) - Number(a.mine));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [effects, viewerSeat, display]);

  const visible = rows.length > 0;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!visible || !el) return undefined;
    const root = document.documentElement;
    const publish = () => {
      root.style.setProperty("--effects-strip-height", `${Math.ceil(el.getBoundingClientRect().height)}px`);
    };
    root.dataset.effectsStrip = "on";
    publish();
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(publish) : null;
    observer?.observe(el);
    window.addEventListener("resize", publish);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", publish);
      delete root.dataset.effectsStrip;
      root.style.removeProperty("--effects-strip-height");
    };
  }, [visible]);

  if (!visible) return null;
  return (
    <div ref={ref} className="effects-strip" data-testid="active-effects-strip" data-open={open || undefined}>
      <button
        type="button"
        className="effects-strip__line"
        data-testid="active-effects-toggle"
        aria-expanded={open}
        aria-controls="active-effects-details"
        onClick={() => setOpen((value) => !value)}
      >
        <span className="effects-strip__icon" aria-hidden="true">
          ✦
        </span>
        <span className="effects-strip__label">
          Active effects <span className="effects-strip__count">{rows.length}</span>
        </span>
        <span className="effects-strip__chips">
          {rows.map(({ key, effect, text, mine }) => (
            <span
              key={key}
              className="effects-strip__chip"
              data-mine={mine || undefined}
              data-testid="active-effect-chip"
              data-effect={effect.effect}
            >
              {display(effect.seat).position != null && <SeatBadge position={display(effect.seat).position!} />}
              <span className="effects-strip__chip-title">{text.title}</span>
            </span>
          ))}
        </span>
        <span className="effects-strip__chevron" aria-hidden="true">
          {open ? "▴" : "▾"}
        </span>
      </button>
      {open && (
        <ul className="effects-strip__details" id="active-effects-details" data-testid="active-effects-details">
          {rows.map(({ key, effect, text }) => (
            <li key={key} className="effects-strip__row" data-testid="active-effect-row">
              <span className="effects-strip__row-head">
                {display(effect.seat).position != null && <SeatBadge position={display(effect.seat).position!} />}
                <strong>{text.title}</strong>
                <span className="effects-strip__seat">{seatLabel(effect.seat)}</span>
              </span>
              <span className="effects-strip__text">
                <span className="effects-strip__summary">{text.summary}</span>
                <span className="effects-strip__expiry"> · {text.expiry}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};
