import type {
  BattleOfferView,
  BattleRecordView,
  BattleSideView,
  BattleTableView,
  OddsView,
  OutcomeView,
} from "../../model";
import { forceText, plural, unitName } from "../../model";
import {
  Button,
  Card,
  ChipTabs,
  Die,
  Eyebrow,
  Hint,
  Icon,
  InlineNote,
  Offer,
  Pill,
  cx,
} from "../../ui";
import { ActionButton, useDispatch, useSeat, useSeats } from "../context";

/** `lines`: which header lines the table has. Both sides keep them, so their unit rows start level. */
function Side({
  side,
  lines,
  odds,
}: {
  side: BattleSideView;
  lines: { meta: boolean; cards: boolean };
  /** Simulated result for this side, before the roll. */
  odds?: { wins: number; left: string };
}) {
  const dispatch = useDispatch();
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
        <strong>{seat.name}</strong>
        <span className="text-muted">
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
      {lines.meta && (
        <div className="mt-0.5 flex h-[18px] gap-x-3 overflow-hidden text-xs whitespace-nowrap text-muted">
          {side.meta.map((item) => (
            <span
              key={item.text}
              className={cx(
                item.tone === "bad" && "text-red",
                item.tone === "accent" && "text-accent",
              )}
            >
              {item.text}
            </span>
          ))}
        </div>
      )}
      {lines.cards && (
        <div className="h-[18px] truncate text-xs text-muted" title={side.cards}>
          {side.cards}
        </div>
      )}
      {(lines.meta || lines.cards) && <div className="h-[5px]" />}
      {side.rows.length === 0 && (
        <div className="min-h-[34px] border-t border-line/50 py-[3px] text-sm leading-7 text-faint">
          No units
        </div>
      )}
      {side.rows.map((row) => (
        <div
          key={row.unit}
          className={cx(
            "grid min-h-[34px] grid-cols-subgrid items-center gap-y-1.5 border-t border-line/50 py-[3px] text-sm",
            row.gone && "text-faint",
          )}
        >
          <span className="flex min-w-0 items-center gap-[7px] font-mid">
            <Icon name={row.unit} className="size-[18px] text-icon" />
            {unitName(row.unit)}
          </span>
          <span className="tabular-nums">×{row.count}</span>
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
          {row.assign.length > 0 && (
            <div className="col-span-full flex flex-wrap gap-1.5 pb-1 pl-[25px]">
              {row.assign.map((pick) => (
                <Button
                  key={pick.kind}
                  size="sm"
                  disabled={pick.disabled}
                  onClick={() => dispatch({ type: "stageHit", unit: row.unit, kind: pick.kind })}
                >
                  {pick.label}
                </Button>
              ))}
            </div>
          )}
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
    <>
      <div className="overflow-hidden rounded-lg border border-line">
        <div className="flex flex-wrap items-center gap-x-3.5 gap-y-1 border-b border-line bg-white/[.016] px-3 py-1.5 text-sm">
          <strong>{view.label}</strong>
          <span className="text-muted">{view.score}</span>
          {view.hint && <Hint label={`Rules: ${view.label}`}>{view.hint}</Hint>}
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
              lines={{
                meta: view.sides.some((item) => item.meta.length > 0),
                cards: view.sides.some((item) => item.cards),
              }}
            />
          ))}
        </div>
      </div>
      {view.notes.map((note) => (
        <InlineNote key={note} tone="quiet">
          {note}
        </InlineNote>
      ))}
    </>
  );
}

export function OutcomeCard({ view }: { view: OutcomeView }) {
  const seats = useSeats();
  return (
    <>
      <Card className="border-green/20! bg-green/[.024]! px-3.5 py-3">
        <h3 className="text-md font-semibold">{view.title}</h3>
        <p className="text-xs text-muted">{view.subtitle}</p>
        <div className="mt-2.5 grid grid-cols-2 gap-x-4 gap-y-1.5 border-t border-green/15 pt-2.5 text-sm">
          {view.sides.map((side) => (
            <div key={side.seat}>
              <Eyebrow className="block text-faint">{seats[side.seat].faction} losses</Eyebrow>
              {forceText(side.lost)}
              <Eyebrow className="mt-1.5 block text-faint">{side.keptLabel}</Eyebrow>
              {forceText(side.kept)}
            </div>
          ))}
        </div>
      </Card>
      {view.note && <InlineNote>{view.note}</InlineNote>}
    </>
  );
}

/** Cards, warnings and choices that belong to the open stage of a battle. */
export function BattleOffers({ offers }: { offers: BattleOfferView[] }) {
  return (
    <>
      {offers.map((offer) =>
        offer.kind === "note" ? (
          <InlineNote
            key={offer.strong}
            tone={offer.tone}
            strong={
              <>
                {offer.alert && <Icon name="alert" />} {offer.strong}
              </>
            }
          >
            {offer.text}
            {offer.hint && <Hint label={`Rules: ${offer.strong}`}>{offer.hint}</Hint>}
          </InlineNote>
        ) : (
          <Offer
            key={offer.title}
            eyebrow={offer.eyebrow}
            title={offer.title}
            hint={offer.text && <Hint label={`About ${offer.title}`}>{offer.text}</Hint>}
            actions={
              offer.actions.length
                ? offer.actions.map((action) => <ActionButton key={action.label} view={action} />)
                : undefined
            }
          />
        ),
      )}
    </>
  );
}

/** The rolls of one battle, newest last. `selected` falls back to the latest record. */
export function BattleRecords({
  label,
  records,
  selected,
  onSelect,
  offers,
}: {
  label: string;
  records: BattleRecordView[];
  selected: string | null;
  onSelect: (key: string) => void;
  offers?: BattleOfferView[];
}) {
  if (!records.length) {
    return offers?.length ? <BattleOffers offers={offers} /> : null;
  }
  const latest = records[records.length - 1];
  const record = records.find((item) => item.key === selected) ?? latest;
  return (
    <>
      {record === latest && offers && <BattleOffers offers={offers} />}
      <ChipTabs
        label={label}
        options={records.map((item) => ({ id: item.key, label: item.label }))}
        value={record.key}
        onChange={onSelect}
      />
      <BattleTable view={record.table} odds={record.odds} />
    </>
  );
}
