import React, { useState } from "react";
import {
  describeFaction,
  describeUnit,
  leaderStatus,
  resolveUnit,
  type FactionInfo,
  type LeaderLine,
  type UnitDescription,
} from "../presentation/factionInfo.ts";
import { useSeatInfo } from "../presentation/SeatInfoContext.tsx";
import { Popover } from "../primitives/index.ts";
import { InfoPopover } from "./InfoPopover.tsx";
import { CostValue } from "./PlanetValueIcons.tsx";
import { UnitIcon } from "./UnitIcon.tsx";

const NO_INFO = "No information available for this in the game content.";

/** One unit's card: numbers, abilities and the printed text. */
export const UnitInfoCard: React.FC<{ unit: UnitDescription }> = ({ unit }) => (
  <div className="info-card__body" data-testid="unit-info-card">
    <div className="info-card__title">
      {unit.name}
      <span className="info-card__tag">{unit.typeLabel}</span>
      {unit.factionSpecific && <span className="info-card__tag">faction</span>}
      {unit.upgraded && <span className="info-card__tag">upgraded</span>}
    </div>
    {unit.subtitle && <div className="info-card__sub">{unit.subtitle}</div>}
    {unit.facts.length > 0 && (
      <dl className="info-card__facts">
        {unit.facts.map((fact) => (
          <div key={fact.label} className="info-card__fact">
            <dt>{fact.label}</dt>
            <dd>{fact.label === "Cost" ? <CostValue text={fact.value} /> : fact.value}</dd>
          </div>
        ))}
      </dl>
    )}
    {unit.keywords.length > 0 && (
      <ul className="info-card__keywords" aria-label="Abilities">
        {unit.keywords.map((keyword) => (
          <li key={keyword}>{keyword}</li>
        ))}
      </ul>
    )}
    {unit.text.map((line) => (
      <p key={line} className="info-card__text">
        {line}
      </p>
    ))}
  </div>
);

/**
 * An info button for a unit: opens its card for the given seat (default: the viewer), so a faction's
 * own flagship, mech and variants and the unit upgrades the seat owns replace the generic card.
 * A unit the content does not know opens a plain "no information" card.
 */
export const UnitInfoButton: React.FC<{
  unit: string;
  seat?: string | null;
  /** The unit's display name, for the accessible label. */
  name?: string;
  testId?: string;
}> = ({ unit, seat, name, testId }) => {
  const { faction, technologies } = useSeatInfo(seat);
  const resolved = resolveUnit(unit, faction, technologies);
  const description = resolved ? describeUnit(resolved) : undefined;
  const shown = description?.name ?? name ?? unit;
  return (
    <InfoPopover
      label={`${shown}: unit details`}
      data-testid={testId ?? `unit-info-${unit}`}
      className="info-card--unit"
    >
      {description ? (
        <UnitInfoCard unit={description} />
      ) : (
        <div className="info-card__body" data-testid="unit-info-empty">
          <div className="info-card__title">{shown}</div>
          <p className="info-card__text">{NO_INFO}</p>
        </div>
      )}
    </InfoPopover>
  );
};

/**
 * Hover (and keyboard focus) card with the unit's numbers and abilities, for a unit of any seat:
 * the owner's faction and unit upgrades decide which card applies. Wraps the unit's icon and name.
 * A unit the content does not know shows nothing.
 */
export const UnitStatsTip: React.FC<{
  unit: string;
  seat?: string | null;
  children: React.ReactNode;
  testId?: string;
}> = ({ unit, seat, children, testId }) => {
  const { faction, technologies } = useSeatInfo(seat);
  const [open, setOpen] = useState(false);
  const resolved = resolveUnit(unit, faction, technologies);
  const description = resolved ? describeUnit(resolved) : undefined;
  if (!description) return <>{children}</>;
  // Portalled, so a scrolling panel or dialog body cannot clip the card.
  return (
    <Popover
      portal
      isOpen={open}
      onOpenChange={setOpen}
      position="top"
      ariaLabel={`${description.name}: unit details`}
      className="info-card info-card--unit"
      data-testid={testId ?? `unit-stats-tip-${unit}`}
      content={<UnitInfoCard unit={description} />}
    >
      <span
        tabIndex={0}
        data-unit-stats={unit}
        style={{ display: "inline-flex", alignItems: "center", gap: 6 }}
        onPointerEnter={(e) => e.pointerType !== "touch" && setOpen(true)}
        onPointerLeave={(e) => e.pointerType !== "touch" && setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
      >
        {children}
      </span>
    </Popover>
  );
};

/** One line of numbers for a unit of any seat: "Cost 3 · Combat 9 · Move 1 · Sustain damage". */
export const UnitStatLine: React.FC<{ unit: string; seat?: string | null }> = ({ unit, seat }) => {
  const { faction, technologies } = useSeatInfo(seat);
  const resolved = resolveUnit(unit, faction, technologies);
  if (!resolved) return null;
  const card = describeUnit(resolved);
  const parts = [...card.facts.map((fact) => `${fact.label} ${fact.value}`), ...card.keywords];
  return parts.length ? <span data-testid="unit-stat-line">{parts.join(" · ")}</span> : null;
};

/** Icon, name and info button of the seat's mech; nothing when the faction has no mech card. */
export const MechInfoRow: React.FC<{ seat?: string | null }> = ({ seat }) => {
  const { faction, technologies } = useSeatInfo(seat);
  const resolved = resolveUnit("mech", faction, technologies);
  if (!resolved?.factionSpecific) return null;
  const mech = describeUnit(resolved);
  return (
    <div className="info-row" data-testid="mech-info-row">
      <UnitIcon type="mech" size={16} />
      <span>{mech.name}</span>
      <UnitInfoButton unit="mech" seat={seat} name={mech.name} testId="mech-info" />
    </div>
  );
};

const LEADER_LABEL: Record<LeaderLine["type"], string> = {
  agent: "Agent",
  commander: "Commander",
  hero: "Hero",
};

const Leader: React.FC<{ leader: LeaderLine; status?: string }> = ({ leader, status }) => (
  <li className="info-card__leader" data-testid={`faction-leader-${leader.type}`}>
    <div className="info-card__leader-head">
      <strong>{LEADER_LABEL[leader.type] ?? leader.type}</strong> {leader.name}
      {status && <span className="info-card__tag">{status}</span>}
    </div>
    {(leader.title || leader.abilityName) && (
      <div className="info-card__sub">{leader.abilityName ?? leader.title}</div>
    )}
    {leader.window && <p className="info-card__text info-card__window">{leader.window}</p>}
    {leader.text && <p className="info-card__text">{leader.text}</p>}
    {leader.unlock && leader.unlock !== "Always Unlocked" && (
      <p className="info-card__text info-card__unlock">Unlock: {leader.unlock}</p>
    )}
  </li>
);

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <section className="info-card__section">
    <h4 className="info-card__heading">{title}</h4>
    {children}
  </section>
);

/** A faction's card: abilities, promissory note, flagship, mech, technologies and leaders. */
export const FactionInfoCard: React.FC<{
  faction: FactionInfo;
  /** Leader state as the server reports it for this seat; omitted states are simply not shown. */
  leaders?: Record<string, string>;
  /** Whether the seat holds its breakthrough; omitted when unknown (a faction picker, not a seat). */
  breakthroughHeld?: boolean;
}> = ({ faction, leaders, breakthroughHeld }) => (
  <div className="info-card__body" data-testid="faction-info-card">
    <div className="info-card__title">{faction.name}</div>
    {faction.abilities.length > 0 && (
      <Section title="Abilities">
        {faction.abilities.map((ability) => (
          <div key={ability.id} className="info-card__entry" data-testid="faction-ability">
            <strong>{ability.name}</strong>
            {ability.permanentEffect && <p className="info-card__text">{ability.permanentEffect}</p>}
            {ability.window && <p className="info-card__text info-card__window">{ability.window}</p>}
            {ability.windowEffect && <p className="info-card__text">{ability.windowEffect}</p>}
          </div>
        ))}
      </Section>
    )}
    {faction.promissoryNotes.length > 0 && (
      <Section title="Promissory note">
        {faction.promissoryNotes.map((note) => (
          <div key={note.id} className="info-card__entry" data-testid="faction-note">
            <strong>{note.name}</strong>
            {note.text && <p className="info-card__text">{note.text}</p>}
          </div>
        ))}
      </Section>
    )}
    {faction.breakthrough && (
      <Section title="Breakthrough">
        <div className="info-card__entry" data-testid="faction-breakthrough">
          <strong>{faction.breakthrough.name}</strong>
          {breakthroughHeld !== undefined && (
            <span className="info-card__window"> {breakthroughHeld ? "unlocked" : "locked"}</span>
          )}
          <p className="info-card__text info-card__window">Synergy: {faction.breakthrough.synergy}</p>
          {faction.breakthrough.text && (
            <p className="info-card__text" style={{ whiteSpace: "pre-line" }}>
              {faction.breakthrough.text}
            </p>
          )}
        </div>
      </Section>
    )}
    {faction.flagship && (
      <Section title="Flagship">
        <UnitInfoCard unit={faction.flagship} />
      </Section>
    )}
    {faction.mech && (
      <Section title="Mech">
        <UnitInfoCard unit={faction.mech} />
      </Section>
    )}
    {(faction.startingTech.length > 0 || faction.factionTech.length > 0) && (
      <Section title="Technology">
        {faction.startingTech.length > 0 && (
          <p className="info-card__text" data-testid="faction-starting-tech">
            <strong>Starting:</strong> {faction.startingTech.map((t) => t.name).join(", ")}
          </p>
        )}
        {faction.factionTech.map((tech) => (
          <div key={tech.id} className="info-card__entry" data-testid="faction-tech">
            <strong>{tech.name}</strong>
            {tech.description && <p className="info-card__text">{tech.description}</p>}
          </div>
        ))}
      </Section>
    )}
    {faction.leaders.length > 0 && (
      <Section title="Leaders">
        <ul className="info-card__leaders">
          {faction.leaders.map((leader) => (
            <Leader key={leader.id} leader={leader} status={leaderStatus(leaders, leader.id)} />
          ))}
        </ul>
      </Section>
    )}
  </div>
);

/**
 * The "Faction" button of a player sheet: opens the faction's card. A faction the content does not
 * know opens a plain "no information" card.
 */
export const FactionInfoButton: React.FC<{
  faction: string;
  technologies?: readonly string[];
  leaders?: Record<string, string>;
  /** The seat's earned breakthrough alias, or null while it is locked; omit outside a seat. */
  breakthroughHeld?: string | null;
}> = ({ faction, technologies, leaders, breakthroughHeld }) => {
  const info = describeFaction(faction, technologies);
  return (
    <InfoPopover
      label={`${info?.name ?? faction}: faction details`}
      trigger="Faction"
      className="info-card--faction"
      data-testid="faction-info-button"
    >
      {info ? (
        <FactionInfoCard
          faction={info}
          leaders={leaders}
          breakthroughHeld={breakthroughHeld === undefined ? undefined : breakthroughHeld !== null}
        />
      ) : (
        <div className="info-card__body" data-testid="faction-info-empty">
          <div className="info-card__title">{faction}</div>
          <p className="info-card__text">{NO_INFO}</p>
        </div>
      )}
    </InfoPopover>
  );
};
