import type { GroundOddsView, LandingPlanetView, LandingView } from "../../../model";
import { GROUND_ODDS_COUNT, forceText, plural } from "../../../model";
import { Button, Hint, Icon, Press, cx, useReadOnly } from "../../../ui";
import { PlanetTraits } from "../../board/PlanetTraits";
import { BattleTable } from "../../combat/BattleTable";
import { CombatOdds } from "../../combat/SpaceOdds";
import { OwnerLabel, SeatSymbol, useDispatch, useSeat } from "../../context";
import { useLink } from "../../link";
import { SourceChip } from "../movement/Tokens";

/**
 * The odds of the ground combat that the staged units start on a planet, as `ti4-calc` computes
 * them in the page. A defense that the odds cannot count has "No odds".
 */
function GroundOdds({ odds }: { odds: GroundOddsView }) {
  const defender = useSeat(odds.defender);
  if (!odds.unavailable) {
    return (
      <CombatOdds view={odds} label="Ground combat" className="text-xs">
        Simulated{odds.rollouts > 0 ? ` · ${odds.rollouts} rollouts` : ""}. Win: the ground forces
        that land take the planet. Draw: both sides are destroyed, and the planet stays with the
        defender. Survivors: the units that are expected to be left, of the units that fight.{" "}
        {GROUND_ODDS_COUNT}
      </CombatOdds>
    );
  }
  return (
    <section aria-label="Ground combat" className="flex min-w-0 items-baseline gap-1.5 text-xs">
      <strong className="font-mid whitespace-nowrap">Ground combat</strong>
      <span className="text-muted">·</span>
      <SeatSymbol seat={odds.defender} />
      <span className="min-w-0 truncate text-muted" title={defender?.name}>
        {defender?.faction ?? odds.defender}
      </span>
      <span className="ml-auto whitespace-nowrap text-muted">No odds</span>
      <Hint label="About the odds">
        The planet has a defense that the simulation cannot count, so there are no odds.
      </Hint>
    </section>
  );
}

/** One planet: who holds it, what is there, then a token for each unit that lands and a free slot. */
function PlanetRow({ view, planet }: { view: LandingView; planet: LandingPlanetView }) {
  const dispatch = useDispatch();
  const { linked, props } = useLink([`pl:${planet.id}`]);
  const chosen = view.forces.find((force) => force.id === view.chosen);
  const accept = chosen ? planet.accepts[chosen.id] : undefined;
  const box =
    "relative grid size-8 flex-none place-items-center rounded-md border touch:size-10 disabled:opacity-45";
  return (
    <div
      className={cx(
        "flex flex-col gap-1 border-line/50 px-3 py-1.5 not-first:border-t phone:px-2.5",
        linked && "bg-cyan/[.06]",
      )}
      {...props}
    >
      <div className="flex flex-wrap items-baseline gap-x-2 text-xs text-muted">
        <strong className="text-sm font-mid text-text">{planet.name}</strong>
        <span className="tabular-nums">
          {planet.resources} / {planet.influence}
        </span>
        <PlanetTraits name={planet.name} traits={planet.traits} />
        <OwnerLabel seat={planet.owner} />
        {planet.there.map((item) => (
          <span key={item.seat} className="inline-flex items-baseline gap-1">
            <SeatSymbol seat={item.seat} />
            {forceText(item.force)}
          </span>
        ))}
      </div>
      {planet.reason ? (
        <span className="text-xs text-faint">{planet.reason}</span>
      ) : (
        <div className="flex flex-wrap items-center gap-[3px]">
          {planet.tokens.map((token, index) => (
            <button
              key={index}
              type="button"
              className={cx(box, "border-accent bg-accent/15 text-text")}
              aria-label={`Take back ${token.name} from ${planet.name}`}
              title={`${token.name} lands · tap to take it back`}
              onClick={() => dispatch({ type: "setCount", key: token.key, value: token.value })}
            >
              <Icon name={token.unit} className="size-5" />
            </button>
          ))}
          <button
            type="button"
            className={cx(box, "border-dashed border-line-strong text-icon hover:bg-raised")}
            disabled={!accept}
            aria-label={`Land ${chosen?.name ?? "a unit"} on ${planet.name}`}
            title={
              accept
                ? `Land ${chosen!.name}`
                : chosen
                  ? `${chosen.name} cannot land here`
                  : "Nothing left to land"
            }
            onClick={() => accept && dispatch({ type: "setCount", ...accept })}
          >
            {/* A sign, not only the outline: this slot takes one more unit. */}
            <span aria-hidden="true">+</span>
          </button>
        </div>
      )}
      {planet.odds && <GroundOdds odds={planet.odds} />}
    </div>
  );
}

/**
 * The landings of an invasion with their controls, on the board, at the active system. On a
 * phone it is a sheet over the lower part of the map.
 */
export function LandingSheet({ view }: { view: LandingView }) {
  const dispatch = useDispatch();
  // The place is said once, before the chips of that place.
  const placeOf = (index: number) =>
    `${view.forces[index].site?.system ?? ""}:${view.forces[index].place}`;
  const opens = (index: number) => index === 0 || placeOf(index - 1) !== placeOf(index);
  return (
    <div
      role="group"
      aria-label={`Land in ${view.label}`}
      className="absolute bottom-2.5 left-2.5 z-2 max-h-[62%] w-[430px] overflow-auto rounded-lg border border-line bg-surface/97 text-sm phone:inset-x-0 phone:bottom-0 phone:z-3 phone:max-h-[70%] phone:w-auto phone:rounded-none phone:border-x-0 phone:border-b-0 phone:bg-surface"
    >
      <div className="sticky top-0 z-1 bg-surface">
        <div className="flex items-center gap-2 border-b border-line py-0.5 pr-1 pl-3 phone:pl-2.5">
          <h3 className="truncate text-base font-semibold" title={view.label}>
            {view.label}
          </h3>
          <span className="min-w-0 flex-1 truncate text-xs text-muted">
            {view.landed > 0 ? (
              <strong className="font-mid text-text">{plural(view.landed, "unit")} land</strong>
            ) : (
              "Nothing lands"
            )}
          </span>
          <Button
            size="sm"
            tone="quiet"
            disabled={view.landed === 0}
            onClick={() => dispatch({ type: "resetOrigin", system: view.system })}
          >
            Reset
          </Button>
          {/* Closes the sheet. What is staged stays staged; the main button sends it. */}
          <button
            type="button"
            aria-label="Done with the landings"
            title="Done with the landings · closes the sheet"
            className="grid size-7 flex-none place-items-center rounded-md border border-line-strong text-icon hover:bg-raised touch:size-9"
            onClick={() => dispatch({ type: "markOrigin", system: view.system })}
          >
            <Icon name="check" />
          </button>
        </div>
        <div
          role="group"
          aria-label="Land"
          className="flex items-center gap-1.5 border-b border-line py-1 pr-1.5 pl-3 phone:pl-2.5"
        >
          <span className="text-xs text-muted phone:hidden">Land</span>
          <span className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none]">
            {view.forces.map((force, index) => (
              <span key={force.id} className="contents">
                {opens(index) && (
                  <span
                    className={cx(
                      "flex-none text-xs whitespace-nowrap text-muted",
                      index > 0 && "ml-1 border-l border-line pl-2",
                    )}
                  >
                    {force.site && `#${force.site.system} `}
                    {force.place}
                  </span>
                )}
                <SourceChip
                  verb="Land"
                  source={force}
                  pressed={force.id === view.chosen}
                  onSelect={() =>
                    dispatch({ type: "chooseOption", option: force.id, group: "landing" })
                  }
                />
              </span>
            ))}
          </span>
        </div>
      </div>
      {view.planets.map((planet) => (
        <PlanetRow key={planet.id} view={view} planet={planet} />
      ))}
    </div>
  );
}

/** In the panel: what lands on each planet. The controls are on the board. */
export function LandingSummary({ view }: { view: LandingView }) {
  const dispatch = useDispatch();
  const row =
    "row flex min-h-8 w-full flex-wrap items-baseline gap-x-2.5 px-3.5 py-1 text-left hover:bg-raised [.row+&]:border-t [.row+&]:border-line/50 phone:px-2.5";
  const open = () => dispatch({ type: "inspectSystem", system: view.system });
  // Read-only, the landings are made: what landed on each planet, nothing to choose.
  const readOnly = useReadOnly();
  return (
    <div className="rounded-lg border border-line">
      {!readOnly && (
        <div className="border-b border-line px-3.5 py-1.5 text-xs text-muted phone:px-2.5">
          Choose a planet on the map to land ground forces.
        </div>
      )}
      {view.planets.map((planet) => (
        <Press key={planet.id} className={row} onClick={open}>
          <strong className="font-mid">{planet.name}</strong>
          <span className="text-xs text-muted">
            {planet.lands ? (
              <strong className="font-mid text-text">{planet.lands}</strong>
            ) : (
              (planet.reason ?? (readOnly ? "Nothing landed" : "Nothing lands"))
            )}
          </span>
          {planet.odds && !planet.odds.preview && (
            <span className="ml-auto text-xs whitespace-nowrap text-muted">
              Ground combat · {planet.odds.unavailable ? "No odds" : "Win …"}
            </span>
          )}
        </Press>
      ))}
      {!readOnly && (
        <Press className={row} onClick={open}>
          <strong className="font-mid">Space area</strong>
          <span className="text-xs text-muted">
            {view.stays ? `${view.stays} · not landed` : "Every ground force lands"}
          </span>
        </Press>
      )}
    </div>
  );
}

/**
 * In the panel: the first round of each ground combat that the landings start, with the odds in
 * the headers of the sides. While the answer for new landings is on its way, the table of the
 * last one is dimmed.
 */
export function GroundPreviews({ view }: { view: LandingView }) {
  return (
    <>
      {view.planets.map(
        ({ id, odds }) =>
          odds?.preview && (
            <section
              key={id}
              aria-label={odds.preview.table.label}
              aria-busy={odds.pending}
              className={cx(odds.pending && "opacity-50")}
            >
              <BattleTable view={odds.preview.table} odds={odds.preview.odds} />
            </section>
          ),
      )}
    </>
  );
}
