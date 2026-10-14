import { useEffect, useState } from "react";
import type { CombatView, InvasionPlanetView, InvasionView } from "../../model";
import { unitName } from "../../model";
import { Card, CardBody, CardHeading, Counter, Gauge, Gauges, InlineNote, ListRow } from "../../ui";
import { BattleOffers, BattleRecords, BattleTable, OutcomeCard } from "../combat/BattleTable";
import { OwnerLabel, useDispatch } from "../context";
import { LandingSummary } from "./invasion/LandingSheet";

/** Which roll is in view. It goes back to the latest roll when the battle moves on. */
function useRecordTab(revision: string) {
  const [selected, setSelected] = useState<Record<string, string>>({});
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new revision is the trigger
  useEffect(() => setSelected({}), [revision]);
  return [
    selected,
    (id: string, key: string) => setSelected((now) => ({ ...now, [id]: key })),
  ] as const;
}

export function Skipped({ step, reason }: { step: string; reason: string }) {
  return (
    <InlineNote tone="quiet" strong={`${step} skipped.`}>
      {reason}
    </InlineNote>
  );
}

/** Step 3: odds before the roll, then one table for each roll, and the decisions of the open round. */
export function SpaceCombatStep({ view }: { view: CombatView }) {
  const [selected, select] = useRecordTab(view.revision);
  if (view.skipped) {
    return <Skipped step="Space combat" reason={view.skipped} />;
  }
  return (
    <>
      {view.outcome && <OutcomeCard view={view.outcome} />}
      <BattleRecords
        label="Rolls"
        records={view.records}
        selected={selected.space ?? null}
        onSelect={(key) => select("space", key)}
        offers={view.offers}
      />
    </>
  );
}

function Planet({
  planet,
  selected,
  onSelect,
}: {
  planet: InvasionPlanetView;
  selected: string | null;
  onSelect: (key: string) => void;
}) {
  const dispatch = useDispatch();
  return (
    <Card>
      <CardHeading title={planet.name}>{planet.heading}</CardHeading>
      {planet.counters.map((item) => (
        <ListRow
          key={item.unit}
          icon={item.unit}
          title={unitName(item.unit)}
          subtitle={`${item.left} left in space`}
          controls={
            <Counter
              {...item.counter}
              onChange={(value) => dispatch({ type: "setCount", key: item.counter.id, value })}
            />
          }
        />
      ))}
      <CardBody>
        <div className="flex flex-wrap gap-x-3.5 gap-y-[3px] text-sm text-muted">
          <OwnerLabel seat={planet.owner} />
          {planet.facts.map((fact, index) => (
            <span key={index}>
              {fact.label}
              {fact.strong && <strong className="font-mid text-text">{fact.strong}</strong>}
              {fact.text}
            </span>
          ))}
        </div>
        {planet.outcome && <OutcomeCard view={planet.outcome} />}
        {planet.result && (
          <p
            className={
              planet.result.failed
                ? "text-sm font-semibold text-red"
                : "text-sm font-semibold text-green"
            }
          >
            {planet.result.text}
          </p>
        )}
        {planet.projection &&
          ("odds" in planet.projection ? (
            <BattleTable view={planet.projection.table} odds={planet.projection.odds} />
          ) : (
            <InlineNote tone="quiet">{planet.projection.note}</InlineNote>
          ))}
        <BattleOffers offers={planet.offers} />
        {planet.preview ? (
          <BattleTable view={planet.preview} />
        ) : (
          <BattleRecords
            label="Rolls"
            records={planet.records}
            selected={selected}
            onSelect={onSelect}
          />
        )}
      </CardBody>
    </Card>
  );
}

/** Step 4: one card for each planet: who holds it, what lands, and every roll there. */
export function InvasionStep({ view }: { view: InvasionView }) {
  const [selected, select] = useRecordTab(view.revision);
  if (view.skipped) {
    return <Skipped step="Invasion" reason={view.skipped} />;
  }
  if (view.landing) {
    return <LandingSummary view={view.landing} />;
  }
  return (
    <>
      {view.planets.map((planet) => (
        <Planet
          key={planet.id}
          planet={planet}
          selected={selected[planet.id] ?? null}
          onSelect={(key) => select(planet.id, key)}
        />
      ))}
      {view.gauges.length > 0 && (
        <Gauges>
          {view.gauges.map((gauge) => (
            <Gauge key={gauge.label} {...gauge} />
          ))}
        </Gauges>
      )}
      {view.plannedNote && (
        <InlineNote tone="quiet">
          These landings are planned. Their results resolve in Live.
        </InlineNote>
      )}
    </>
  );
}
