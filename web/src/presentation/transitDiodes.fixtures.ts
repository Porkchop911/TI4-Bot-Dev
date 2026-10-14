// Transit Diodes options shaped like the engine's, for tests.
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { actor } from "../dev/decisionGalleryCases.ts";

export const transitOption = (ss: string, source: string, unit: string, ds: string, planet: string): ChoiceOptionDto => ({
  id: `transit|${ss}|${source}|${unit}|${ds}|${planet}`,
  label: `move ${unit} from ${source} to ${planet}`,
  kind: "transit",
  payload: { source_system: ss, source, unit, destination_system: ds, planet },
});

export const transitChoice = (options: ChoiceOptionDto[]): PendingChoiceDto => ({
  actor,
  nonce: "td-1",
  prompt: "Transit Diodes: redeploy ground forces (3 left)",
  context: { subtype: "transit_diodes_redeploy" } as PendingChoiceDto["context"],
  options: [...options, { id: "decline", label: "finish redeployment", kind: "decline" }],
});
