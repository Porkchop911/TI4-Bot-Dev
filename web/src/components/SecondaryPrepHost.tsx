import React, { useLayoutEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useBoardPrepSlot } from "../presentation/BoardPrepSlot.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { describePlan, TRADE_WARNING } from "../presentation/secondaryPlan.ts";
import { describeBlocker, describePayment } from "../presentation/secondaryPreview.ts";
import type { SecondaryPrepare } from "../hooks/useSecondaryPrepare.ts";
import { ValueText } from "./PlanetValueIcons.tsx";
import "./SecondaryPrep.css";

export interface SecondaryPrepHostProps {
  prep: SecondaryPrepare;
  /**
   * Sends the prepared answer for the viewer's real pending decision (the click-equivalent path).
   * Defaults to the preparation hook's own play.
   */
  onConfirm?: (resolution: SecondaryPrepare["resolution"]) => Promise<void>;
  busy?: boolean;
}

/**
 * The chrome around preparing a strategy-card secondary with the usual UI:
 *  - the chip that opens preparation mode, and the "Preparing" banner while it is open;
 *  - the prepared answer for the viewer's real question: a one-click bar (Review), "Needs review",
 *    and the auto-play toasts. The question itself is rendered by the real components.
 * Spectators and non-followers see nothing.
 */
export const SecondaryPrepHost: React.FC<SecondaryPrepHostProps> = ({ prep, onConfirm, busy }) => {
  const display = usePlayerIdentity();
  const slot = useBoardPrepSlot()?.slot ?? null;
  const { action, plan, resolution, realChoice, mine, pending, played, holding } = prep;
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Phone only (the styles ignore it elsewhere): the banner folds to its title and Save plan row, so
  // it stops covering the map while the stand-in question is minimized. Reset whenever it closes.
  const [folded, setFolded] = useState(false);
  const nonce = mine && realChoice ? realChoice.nonce : null;

  const confirm = async () => {
    setSending(true);
    setError(null);
    try {
      await (onConfirm ?? prep.play)(resolution);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setSending(false);
    }
  };

  const showChip = prep.canPrepare && !prep.preparing && action !== null;
  const actionable =
    resolution.kind === "option" ||
    resolution.kind === "tokens" ||
    resolution.kind === "production" ||
    resolution.kind === "payment" ||
    resolution.kind === "ready";
  const showBar =
    nonce !== null &&
    dismissed !== nonce &&
    !pending &&
    !holding &&
    (actionable || resolution.kind === "review");
  const showBanner = prep.preparing && action !== null;
  // The dialog of the stand-in question starts below the banner (see SecondaryPrep.css).
  useLayoutEffect(() => {
    if (!showBanner) {
      setFolded(false);
      return;
    }
    const root = document.documentElement;
    const measure = () => {
      const banner = document.querySelector<HTMLElement>('[data-testid="prepare-banner"]');
      const docked = banner?.closest(".secondary-prep") as HTMLElement | null;
      const fixed = !!docked && getComputedStyle(docked).position === "fixed";
      const bottom = banner && docked && !fixed ? Math.ceil(banner.getBoundingClientRect().bottom) + 8 : 0;
      root.style.setProperty("--prep-banner-bottom", `${bottom}px`);
      // A docked banner: how far its top is from the bottom edge, so the minimized-decision pill sits above it.
      const dock = banner && fixed ? Math.ceil(window.innerHeight - banner.getBoundingClientRect().top) : 0;
      root.style.setProperty("--prep-dock", `${dock}px`);
      // The dialog of the stand-in question keeps clear of a docked banner: below it on a portrait phone,
      // to its left on a landscape phone (where the banner sits at the right edge).
      const rect = banner?.getBoundingClientRect();
      const side = !!rect && fixed && rect.left > 40;
      root.style.setProperty("--prep-pad-bottom", `${rect && fixed && !side ? dock : 0}px`);
      root.style.setProperty("--prep-pad-right", `${rect && side ? Math.ceil(window.innerWidth - rect.left) + 8 : 0}px`);
    };
    measure();
    const banner = document.querySelector('[data-testid="prepare-banner"]');
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    if (banner) observer?.observe(banner);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
      root.style.removeProperty("--prep-banner-bottom");
      root.style.removeProperty("--prep-dock");
      root.style.removeProperty("--prep-pad-bottom");
      root.style.removeProperty("--prep-pad-right");
    };
  }, [showBanner]);
  if (!showChip && !showBanner && !showBar && !pending && !played) return null;

  // The chip sits in the board's toolbar strip (a flow item there); the rest floats below that strip.
  // A preparing banner keeps a same-sized spacer in the strip so the map does not jump when it opens.
  const chipRow = (
    <>
      {showChip && action && (
        <button type="button" className="secondary-prep__chip" data-testid="secondary-prep-chip" onClick={prep.open}>
          {plan ? "Prepared" : "Prepare your secondary"}
          <span className="secondary-prep__note" style={{ margin: 0 }}>
            {plan ? describePlan(plan) : `${action.cardName}, played by ${display(action.primary).label}`}
          </span>
        </button>
      )}
      {showChip && plan && (
        <button type="button" className="button button--secondary button--sm" data-testid="prep-clear-chip" onClick={prep.clear}>
          Clear prepared
        </button>
      )}
      {showBanner && <div className="secondary-prep__chip-spacer" aria-hidden="true" />}
    </>
  );
  const hasChips = showChip || showBanner;
  const hasFloating = showBanner || showBar || !!pending || !!played;
  // Without a board slot (no board mounted) everything floats as before.
  const floatsChips = !slot && hasChips;

  return (
    <>
      {slot && hasChips
        ? createPortal(
            <div className="secondary-prep-chips" data-testid="secondary-prep-chips">
              {chipRow}
            </div>,
            slot,
          )
        : null}
      {(hasFloating || floatsChips) && (
    <div className={`secondary-prep${showBanner ? " secondary-prep--preparing" : ""}`} data-testid="secondary-prep">
      {floatsChips && chipRow}
      {showBanner && action && (
        <section
          className="secondary-prep__banner"
          data-testid="prepare-banner"
          data-folded={folded ? "true" : "false"}
          aria-label="Preparing your secondary"
        >
          <div className="secondary-prep__head">
            <div>
              <span className="secondary-prep__eyebrow">Preparing &mdash; nothing is sent or spent</span>
              <span className="secondary-prep__title">
                {action.cardName} secondary
                <span className="secondary-prep__note" style={{ display: "inline", marginLeft: 8 }}>
                  played by {display(action.primary).label}
                </span>
              </span>
            </div>
            <div className="secondary-prep__row">
              {plan && (
                <span className="secondary-prep__badge" data-testid="secondary-prepared-badge">
                  Prepared
                </span>
              )}
              <button type="button" className="button button--primary button--sm" data-testid="prep-save" onClick={prep.close}>
                Save plan
              </button>
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="prep-clear"
                disabled={!plan}
                onClick={() => {
                  prep.clear();
                  prep.close();
                }}
              >
                Clear plan
              </button>
              <button
                type="button"
                className="button button--secondary button--icon secondary-prep__fold"
                data-testid="prep-fold"
                aria-expanded={!folded}
                aria-label={folded ? "Show details" : "Hide details"}
                onClick={() => setFolded((value) => !value)}
              >
                {folded ? "+" : "−"}
              </button>
            </div>
          </div>
          <p className="secondary-prep__note" data-testid="prepare-plan-summary">
            {plan ? `Your plan: ${describePlan(plan)}. ` : "Answer the question below as you would for real. "}
            <span data-testid="prep-private-note">Private to this device; you can still change your mind when the real question comes.</span>
          </p>
          {prep.dry?.approximate && (
            <details className="secondary-prep__warning" data-testid="prepare-approximate" role="note">
              <summary>Approximate until the real question opens</summary>
              {prep.dry.approximate}
            </details>
          )}
          {prep.dry?.exact && (
            <p className="secondary-prep__note" data-testid="prepare-as-of-now" role="note">
              Exact options from the game as of now; re-checked when your turn comes.
              {action.family === "trade" && ` ${TRADE_WARNING}`}
            </p>
          )}
          {prep.dry?.exact && prep.dry.payment && (
            <p className="secondary-prep__note" data-testid="prepare-payment-preview" role="note">
              Paying the {prep.dry.payment.cost} resources of a research: the cheapest payment is{" "}
              <strong>{describePayment(prep.dry.payment) || "nothing"}</strong>. You choose how to pay when the game
              asks.
            </p>
          )}
          {prep.dry?.exact && action.family === "leadership" && (
            <p className="secondary-prep__note" data-testid="prepare-leadership-payment" role="note">
              Pick the planets and trade goods that pay on the map, as you would for real. The game checks the
              whole purchase, payment included, when you save it and again when your turn comes
              {prep.leadershipCheck?.kind === "unchecked" ? " (it could not be checked just now)" : ""}.
            </p>
          )}
          {prep.leadershipCheck?.kind === "rejected" && (
            <p className="secondary-prep__note secondary-prep__note--attention" data-testid="prepare-leadership-rejected" role="note">
              The game would not accept this purchase as of now: {prep.leadershipCheck.reason}.
            </p>
          )}
          {prep.exactInfo.notAsked && (
            <p className="secondary-prep__note secondary-prep__note--attention" data-testid="prepare-not-asked" role="note">
              {describeBlocker(prep.exactInfo.notAsked)}
            </p>
          )}
        </section>
      )}
      {pending && (
        <div className="secondary-prep__bar" role="status" data-testid="secondary-autoplay-toast">
          <span className="secondary-prep__bar-text">
            Auto-playing your prepared secondary: <strong><ValueText text={pending.text} /></strong>
          </span>
          <span className="secondary-prep__bar-actions">
            <button type="button" className="button button--secondary button--sm" data-testid="secondary-autoplay-cancel" onClick={prep.cancel}>
              Cancel
            </button>
          </span>
        </div>
      )}
      {!pending && played && (
        <div className="secondary-prep__bar" role="status" data-testid="secondary-autoplayed-toast">
          <span className="secondary-prep__bar-text">
            Auto-played your prepared secondary: <strong><ValueText text={played} /></strong>
          </span>
        </div>
      )}
      {showBar && actionable && plan && (
        <div className="secondary-prep__bar" data-testid="secondary-prepared-bar">
          <span className="secondary-prep__bar-text">
            <span className="secondary-prep__badge" style={{ marginRight: 8 }}>Prepared</span>
            <strong>
              <ValueText text={actionable ? resolution.text : ""} />
            </strong>
            {error && <span role="alert"> {error}</span>}
          </span>
          <span className="secondary-prep__bar-actions">
            <button
              type="button"
              className="button button--primary button--sm"
              data-testid="secondary-prepared-confirm"
              disabled={sending || busy}
              onClick={() => void confirm()}
            >
              Confirm
            </button>
            <button
              type="button"
              className="button button--secondary button--sm"
              data-testid="secondary-prepared-dismiss"
              onClick={() => setDismissed(nonce)}
            >
              Choose myself
            </button>
          </span>
        </div>
      )}
      {showBar && resolution.kind === "review" && (
        <div className="secondary-prep__bar secondary-prep__bar--review" data-testid="secondary-needs-review">
          <span className="secondary-prep__bar-text">
            <span className="secondary-prep__badge secondary-prep__badge--review" style={{ marginRight: 8 }}>
              Needs review
            </span>
            {resolution.reason}
            {plan && <span className="secondary-prep__note"> (You prepared: {describePlan(plan)}.)</span>}
          </span>
          <span className="secondary-prep__bar-actions">
            <button type="button" className="button button--secondary button--sm" data-testid="secondary-prepared-clear" onClick={prep.clear}>
              Clear prepared
            </button>
          </span>
        </div>
      )}
    </div>
      )}
    </>
  );
};
