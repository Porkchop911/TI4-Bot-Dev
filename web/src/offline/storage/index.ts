// The save store is shared with web2 (`web-shared/src/storage/`); the feedback queue, the engine
// check dialogs and the import button are web's own.
export * from "@ti4/shared/storage/index.ts";
export {
  createDurableFeedbackQueue,
  httpFeedbackSender,
  backoffDelay,
  FEEDBACK_LIMITS,
  type FeedbackQueueOptions,
  type DurableFeedbackQueue,
  type FeedbackPayload,
  type FeedbackSendResult,
  type FeedbackSender,
  type StoredFeedbackItem,
} from "./feedbackQueue.ts";
export {
  engineMismatchPrompt,
  replayDivergedPrompt,
  type EngineMismatchChoice,
  type EngineMismatchPrompt,
} from "./engineCheck.ts";
export { listSummaries, offlineSummary, shellStatus, storageWarning, firstHumanSeat, humanSeatsOf, NOT_PLAYABLE_HERE } from "./shell.ts";
export { ImportGameButton } from "./ImportGameButton.tsx";
