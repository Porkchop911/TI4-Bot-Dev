import type { ComponentCardView, FactionSheetView } from "../../model";
import { Eyebrow } from "../../ui";
import { PrintedCard } from "../shell/Reference";

function Cards({ title, cards }: { title: string; cards: ComponentCardView[] }) {
  if (cards.length === 0) {
    return null;
  }
  return (
    <section aria-label={title}>
      <h4 className="mt-2.5 text-muted">
        <Eyebrow>{title}</Eyebrow>
      </h4>
      {cards.map((card) => (
        <PrintedCard
          key={card.name}
          name={card.name}
          kind={card.kind}
          window={card.window}
          text={card.text}
        >
          {card.unlock && (
            <div className="mt-0.5 text-xs">
              <span className="text-muted">Unlock: </span>
              {card.unlock}
            </div>
          )}
        </PrintedCard>
      ))}
    </section>
  );
}

/** A faction as its sheet prints it: the values, then the cards in the frame of a player sheet. */
export function FactionSheet({ sheet }: { sheet: FactionSheetView }) {
  return (
    <div className="px-3 py-2.5">
      <strong className="text-sm font-semibold">{sheet.title}</strong>
      <dl className="mt-1.5 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-0.5">
        {sheet.facts.map((fact) => (
          <div key={fact.label} className="col-span-2 grid grid-cols-subgrid">
            <dt className="text-muted">{fact.label}</dt>
            <dd>{fact.value}</dd>
          </div>
        ))}
      </dl>
      <Cards title="Faction abilities" cards={sheet.abilities} />
      <Cards title="Faction technologies" cards={sheet.technologies} />
      <Cards title="Leaders" cards={sheet.leaders} />
    </div>
  );
}
