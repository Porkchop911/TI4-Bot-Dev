import type { MovementView } from "../../model";
import { plural, unitName } from "../../model";
import { Card, Die, Hint, Icon, InlineNote, ListRow, Pill, SectionTitle, cx } from "../../ui";
import { BattleTable } from "../combat/BattleTable";
import { useLink } from "../link";
import { MovementSummary } from "./movement/OriginSheet";
import { Arrival } from "./movement/parts";
import { MovementTokens } from "./movement/Tokens";

function Unreachable({ row }: { row: NonNullable<MovementView["unreachable"]>["rows"][number] }) {
  const { linked, props } = useLink([`sys:${row.system}`]);
  return (
    <span
      className={cx("inline-flex items-center gap-1 rounded-sm", linked && "bg-cyan/[.09]")}
      {...props}
    >
      <Icon name={row.unit} className="size-3.5 text-icon" />
      <strong className="font-mid text-muted">{row.name}</strong>
      {row.text}
    </span>
  );
}

/**
 * Step 2: which ships move to the active system, and what they load. The game chooses the path
 * of a ship. The controls are on the board, at each system; the panel says what leaves.
 */
export function MovementStep({ view }: { view: MovementView }) {
  const Origins = view.editing ? MovementSummary : MovementTokens;
  return (
    <>
      <Arrival view={view} />
      {view.origins.length > 0 && <Origins origins={view.origins} />}
      {view.unreachable && (
        // Dense: the board marks these systems, and the inspector of a system says the same.
        <div className="flex flex-wrap gap-x-4 gap-y-0.5 px-1 text-xs text-faint">
          <strong className="font-mid text-muted">Cannot move to #{view.unreachable.target}</strong>
          {view.unreachable.rows.map((row, index) => (
            <Unreachable key={index} row={row} />
          ))}
        </div>
      )}
      {view.riftRolls > 0 && (
        <InlineNote
          strong={
            <>
              <Icon name="alert" /> {plural(view.riftRolls, "rift roll")}
            </>
          }
        >
          <Hint label="About rift rolls">
            Each ship that leaves a gravity rift is destroyed on 1–3, with its cargo.
          </Hint>
        </InlineNote>
      )}
      {view.rift && (
        <>
          <SectionTitle>Gravity rift</SectionTitle>
          <Card>
            {view.rift.map((ship, index) => (
              <ListRow
                key={index}
                icon={ship.unit}
                title={ship.name}
                subtitle={ship.text}
                invalid={!!ship.roll?.lost}
                controls={
                  ship.roll ? (
                    <>
                      <Die roll={ship.roll.face} hit={!ship.roll.lost} />
                      <Pill tone={ship.roll.lost ? "loss" : "staged"}>
                        {ship.roll.lost ? "Destroyed" : "Survives"}
                      </Pill>
                    </>
                  ) : (
                    <>
                      <Die />
                      <Pill tone="staged">Survives on 4+ · 70%</Pill>
                    </>
                  )
                }
              />
            ))}
          </Card>
        </>
      )}
      {view.removed.length > 0 && (
        <InlineNote>
          Removed after movement: {view.removed.map((unit) => unitName(unit)).join(", ")}.
        </InlineNote>
      )}
      {view.cannon && (
        <>
          <SectionTitle>Space cannon offense</SectionTitle>
          <BattleTable view={view.cannon} />
        </>
      )}
      {view.cannonSkipped && (
        <InlineNote tone="quiet" strong="Space cannon offense skipped.">
          No enemy PDS can reach the active system.
        </InlineNote>
      )}
    </>
  );
}
