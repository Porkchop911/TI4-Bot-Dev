import type { SystemDetailsView } from "../../model";
import { forceText } from "../../model";
import { OwnerLabel } from "../context";
import { PlanetTraits } from "./PlanetTraits";

/**
 * What is in one system: its facts, the ships in space by seat, and each planet with `res/inf`,
 * its traits, its owner and its forces. The inspector shows it, and so does every sheet of a task
 * at a system, so that a click on a system always shows what is in it.
 */
export function SystemDetails({
  view,
  className,
}: {
  view: SystemDetailsView;
  className?: string;
}) {
  return (
    <div className={className}>
      <div className="text-muted">{view.facts}</div>
      {view.notes.map((note) => (
        <div key={note} className="text-muted">
          {note}
        </div>
      ))}
      {view.rows.map((row, index) => (
        <div
          key={index}
          className="mt-[7px] grid grid-cols-[78px_minmax(0,1fr)] gap-2 border-t border-line pt-[7px]"
        >
          <span>
            {row.seat ? (
              <OwnerLabel seat={row.seat} />
            ) : row.planet ? (
              <>
                {row.planet.name}
                <div className="text-muted">
                  {row.planet.resources} / {row.planet.influence}
                </div>
              </>
            ) : (
              row.label
            )}
          </span>
          <span>
            {row.planet && <OwnerLabel seat={row.planet.owner} />}
            {row.planet && (
              <div>
                <PlanetTraits name={row.planet.name} traits={row.planet.traits} />
              </div>
            )}
            {row.text}
            {row.force && forceText(row.force)}
            {row.notes?.map((note) => (
              <div
                key={note.text}
                className={note.tone === "planned" ? "text-accent" : "text-muted"}
              >
                {note.text}
              </div>
            ))}
          </span>
        </div>
      ))}
    </div>
  );
}
