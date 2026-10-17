import { useEffect, useState } from "react";
import type { CombatView, InvasionView } from "../../model";
import { BattleRecords } from "../combat/BattleTable";
import { Blocks } from "../flows/Blocks";
import { GroundPreviews, LandingSummary } from "./invasion/LandingSheet";

/** Which roll is in view. It goes back to the latest roll when the battle moves on. */
function useRecordTab(revision: string) {
  const [selected, setSelected] = useState<string | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new revision is the trigger
  useEffect(() => setSelected(null), [revision]);
  return [selected, setSelected] as const;
}

/** Step 3: one table for each roll, and the decisions of the open round. */
export function SpaceCombatStep({ view }: { view: CombatView }) {
  const [selected, select] = useRecordTab(view.revision);
  return (
    <>
      {view.decision && (
        // The question of the viewer that has no place on the rows: its prompt and its list.
        <section aria-label={view.decision.prompt} className="space-y-2">
          <h3 className="text-base font-semibold">{view.decision.prompt}</h3>
          <Blocks blocks={view.decision.blocks} />
        </section>
      )}
      <BattleRecords label="Rolls" records={view.records} selected={selected} onSelect={select} />
    </>
  );
}

/**
 * Step 4: what lands on each planet, and the first round of each ground combat; once resolved
 * (a turn of the Log), a table for each roll of the invasion.
 */
export function InvasionStep({ view }: { view: InvasionView }) {
  const [selected, select] = useRecordTab(String(view.records?.length ?? 0));
  return (
    <>
      <LandingSummary view={view.landing} />
      <GroundPreviews view={view.landing} />
      {view.records && (
        <BattleRecords label="Rolls" records={view.records} selected={selected} onSelect={select} />
      )}
    </>
  );
}
