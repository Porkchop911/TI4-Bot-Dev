import React from "react";
import { SpaceCombatOverlay, SpaceCombatOverlayProps } from "./SpaceCombatOverlay.tsx";

export type CombatResolutionModalProps = SpaceCombatOverlayProps;

export const CombatResolutionModal: React.FC<CombatResolutionModalProps> = (props) => {
  return <SpaceCombatOverlay {...props} />;
};

export { SpaceCombatOverlay };
