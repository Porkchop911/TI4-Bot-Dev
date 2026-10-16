import type { BattleRecordView, BattleSideView, BattleTableView, OddsView } from "../../model";
import { plural, unitName } from "../../model";
import { ChipTabs, Die, Eyebrow, Hint, Icon, Pill, cx } from "../../ui";
import { useSeat, useSeats } from "../context";

function Side({
  side,
  odds,
}: {
  side: BattleSideView;
  /** Simulated result for this side, before the roll. */
  odds?: { wins: number; left: string };
}) {
  const seat = useSeat(side.seat);
  return (
    <section
      aria-label={side.role}
      className="side grid min-w-0 grid-cols-[minmax(118px,1.3fr)_30px_34px_minmax(64px,2fr)_46px_auto] content-start gap-x-1.5 px-3 pt-[9px] pb-2.5 *:col-span-full [.side+&]:shadow-[-1px_-1px_0_var(--color-line)]"
    >
      <div className="flex items-baseline gap-[7px] text-base">
        <Eyebrow className={side.role === "Attacker" ? "text-cyan" : "text-orange"}>
          {side.role}
        </Eyebrow>
        {/* A long name is cut, with the full name on hover; a phone wraps it. */}
        <strong className="min-w-0 truncate phone:whitespace-normal" title={seat.name}>
          {seat.name}
        </strong>
        <span className="shrink-0 text-muted">
          {seat.faction}
          {side.isViewer ? " · You" : ""}
        </span>
        <span className="ml-auto flex items-baseline gap-3 tabular-nums">
          {odds && (
            <span className="text-sm text-muted" title="Simulated result of this battle">
              <strong className="text-md font-strong text-text">{odds.wins}%</strong> win · ~
              {odds.left} survive
            </span>
          )}
          {side.hits !== null && <span className="font-strong">{plural(side.hits, "hit")}</span>}
        </span>
      </div>
      {side.rows.length === 0 && (
        <div className="min-h-[34px] border-t border-line/50 py-[3px] text-sm leading-7 text-faint">
          No units
        </div>
      )}
      {side.rows.map((row, index) => (
        <div
          key={`${row.unit ?? "row"}-${index}`}
          className="grid min-h-[34px] grid-cols-subgrid items-center gap-y-1.5 border-t border-line/50 py-[3px] text-sm"
        >
          <span className="flex min-w-0 items-center gap-[7px] font-mid">
            {row.unit && <Icon name={row.unit} className="size-[18px] shrink-0 text-icon" />}
            {/* A long printed name is cut, with the full name on hover; a phone wraps it. */}
            <span
              className="min-w-0 truncate phone:whitespace-normal"
              title={row.label ?? (row.unit ? unitName(row.unit) : undefined)}
            >
              {row.label ?? (row.unit ? unitName(row.unit) : "")}
            </span>
          </span>
          <span className="tabular-nums">{row.count === null ? "" : `×${row.count}`}</span>
          <span
            className={cx("tabular-nums", row.modified ? "text-accent" : "text-muted")}
            title={row.targetTitle || undefined}
          >
            {row.target ? `${row.target}+` : "—"}
          </span>
          <span className="flex flex-wrap gap-[3px]">
            {typeof row.dice === "number"
              ? Array.from({ length: row.dice }, (_, index) => <Die key={index} />)
              : row.dice.map((die, index) => <Die key={index} roll={die.roll} hit={die.hit} />)}
          </span>
          <span
            className={cx(
              "text-right whitespace-nowrap tabular-nums",
              row.scored ? "font-strong text-green" : "text-muted",
            )}
          >
            {row.hits}
          </span>
          <span className="flex justify-end gap-1">
            {row.pills.map((pill, index) => (
              <Pill key={index} tone={pill.tone} title={pill.title}>
                {pill.label}
              </Pill>
            ))}
          </span>
        </div>
      ))}
    </section>
  );
}

/** One table for every dice step: space cannon, barrage, combat rounds, bombardment, ground combat. */
export function BattleTable({ view, odds }: { view: BattleTableView; odds?: OddsView }) {
  const seats = useSeats();
  const draw = odds ? 100 - odds.attackerWins - odds.defenderWins : 0;
  return (
    <div className="overflow-hidden rounded-lg border border-line">
      <div className="flex flex-wrap items-center gap-x-3.5 gap-y-1 border-b border-line bg-white/[.016] px-3 py-1.5 text-sm">
        <strong>{view.label}</strong>
        <span className="text-muted">{view.score}</span>
        {odds && (
          <span className="inline-flex items-center gap-2 text-muted">
            Avg {odds.rounds} rounds{draw > 0 ? ` · ${draw}% both destroyed` : ""}
            <Hint label="About the odds">
              Simulated · {odds.rollouts} rollouts · {odds.caveat}
            </Hint>
          </span>
        )}
        {view.owed && (
          <span className="ml-auto font-strong text-accent">
            {plural(view.owed.hits, "hit")} for {seats[view.owed.seat].name} to assign
          </span>
        )}
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(330px,1fr))]">
        {view.sides.map((side) => (
          <Side
            key={side.seat}
            side={side}
            odds={
              odds && {
                wins: side.seat === odds.attacker ? odds.attackerWins : odds.defenderWins,
                left: side.seat === odds.attacker ? odds.attackerLeft : odds.defenderLeft,
              }
            }
          />
        ))}
      </div>
    </div>
  );
}

/** The rolls of one battle, newest last. `selected` falls back to the latest record. */
export function BattleRecords({
  label,
  records,
  selected,
  onSelect,
}: {
  label: string;
  records: BattleRecordView[];
  selected: string | null;
  onSelect: (key: string) => void;
}) {
  if (!records.length) {
    return null;
  }
  const latest = records[records.length - 1];
  const record = records.find((item) => item.key === selected) ?? latest;
  return (
    <>
      <ChipTabs
        label={label}
        options={records.map((item) => ({ id: item.key, label: item.label }))}
        value={record.key}
        onChange={onSelect}
      />
      <BattleTable view={record.table} />
    </>
  );
}
