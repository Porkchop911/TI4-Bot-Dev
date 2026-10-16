import type { ActivationView } from "../../model";
import { plural } from "../../model";
import { Badge, Card, CardHeading, CardText, ListRow, SummaryGrid, SummaryStat } from "../../ui";
import { useDispatch } from "../context";
import { useLink } from "../link";

function Chosen({ chosen }: { chosen: NonNullable<ActivationView["chosen"]> }) {
  const { linked, props } = useLink([`sys:${chosen.system}`]);
  return (
    <>
      <ListRow
        icon="planet"
        title={chosen.label}
        subtitle={chosen.facts}
        controls={<Badge tone="draft">Selected</Badge>}
        linked={linked}
        {...props}
      />
      {chosen.commandToken ? (
        <ListRow
          icon="token"
          title="Your command token is here"
          subtitle="You cannot activate this system again in this round."
          invalid
        />
      ) : (
        <ListRow
          icon="carrier"
          title={`${plural(chosen.shipsInRange, "ship")} in range`}
          subtitle={
            chosen.shipsInRange
              ? `From ${plural(chosen.originsInRange, "system")}`
              : "You can still activate the system."
          }
        />
      )}
      {chosen.production > 0 && <ListRow icon="dock" title={`Production ${chosen.production}`} />}
      {chosen.cannon > 0 && (
        <ListRow icon="pds" title={`Space cannon · ${plural(chosen.cannon, "unit")}`} />
      )}
    </>
  );
}

/** Step 1: choose the system. The board is the picker; the search field is the keyboard path. */
export function ActivationStep({ view }: { view: ActivationView }) {
  const dispatch = useDispatch();
  if (!view.editing) {
    return (
      <SummaryGrid>
        <SummaryStat
          eyebrow="Active system"
          title={view.active ? `#${view.active.system} ${view.active.name}` : "No system chosen"}
        />
        <SummaryStat eyebrow="Cost" title="1 tactic token">
          {view.draft
            ? "Planned · Not spent until you apply"
            : `Spent · Tactic pool ${view.tacticPool[0]} → ${view.tacticPool[1]}`}
        </SummaryStat>
      </SummaryGrid>
    );
  }
  return (
    <Card>
      <CardHeading title="Activation target">Costs 1 tactic token · Choose on the map</CardHeading>
      <label className="flex items-center gap-2.5 border-b border-line px-3.5 py-2 text-xs text-muted">
        <span>Find system</span>
        <input
          type="search"
          list="system-list"
          placeholder="Name or number"
          autoComplete="off"
          className="min-w-0 flex-1 rounded-[5px] border border-line bg-canvas px-2 py-[5px] text-text"
          onChange={(event) => {
            if (view.systems.some((system) => system.label === event.target.value)) {
              dispatch({ type: "findSystem", query: event.target.value });
            }
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              dispatch({ type: "findSystem", query: event.currentTarget.value });
            }
          }}
        />
      </label>
      <datalist id="system-list">
        {view.systems.map((system) => (
          <option key={system.id} value={system.label} />
        ))}
      </datalist>
      {view.chosen ? <Chosen chosen={view.chosen} /> : <CardText>No system chosen.</CardText>}
    </Card>
  );
}
