import React from "react";
import type {
  BarrageWindowModel,
  HitKindModel,
} from "../presentation/hitKind.ts";

/**
 * Names the source of a hit decision and what kind of hit it is (destroy vs normal hit, which
 * ships may take it, whether Sustain Damage applies), above the fleets it is answered on.
 */
export const HitKindBanner: React.FC<{ model: HitKindModel }> = ({ model }) => (
  <section
    className={`hit-kind-banner hit-kind-banner--${model.tone}`}
    data-testid="hit-kind-banner"
    data-cause={model.cause}
    data-kind={model.tone}
    aria-label={`${model.source}: ${model.headline}`}
  >
    <span className="hit-kind-banner__badge">
      <span aria-hidden="true">{model.icon}</span> {model.source}
    </span>
    <strong className="hit-kind-banner__headline">{model.headline}</strong>
    <ul className="hit-kind-banner__chips" aria-label="What kind of hit">
      {model.chips.map((chip) => (
        <li
          key={chip.text}
          className={`hit-kind-chip hit-kind-chip--${chip.tone}`}
        >
          {chip.text}
        </li>
      ))}
    </ul>
  </section>
);

/** The anti-fighter barrage reaction window's explanation. */
export const BarrageWindowBanner: React.FC<{ model: BarrageWindowModel }> = ({
  model,
}) => (
  <section
    className="hit-kind-banner hit-kind-banner--barrage"
    data-testid="barrage-window-banner"
    aria-label={model.headline}
  >
    <span className="hit-kind-banner__badge">
      <span aria-hidden="true">{model.icon}</span> {model.source}
    </span>
    <strong className="hit-kind-banner__headline">{model.headline}</strong>
    {model.lines.map((line) => (
      <p key={line} className="hit-kind-banner__line">
        {line}
      </p>
    ))}
  </section>
);
