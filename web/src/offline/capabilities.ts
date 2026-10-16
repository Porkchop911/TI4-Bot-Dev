import React, { createContext, useContext } from "react";
import type { Capabilities } from "./types.ts";

/** Everything a server-backed game offers. */
export const ONLINE_CAPABILITIES: Capabilities = {
  splicePreview: true,
  turnRedo: true,
  multiplayer: true,
  immediateFeedback: true,
};

/** What a local game offers in v1. */
export const OFFLINE_CAPABILITIES: Capabilities = {
  splicePreview: false,
  turnRedo: false,
  multiplayer: false,
  immediateFeedback: false,
};

const LABELS: Record<keyof Capabilities, string> = {
  splicePreview: "Map splice preview",
  turnRedo: "Turn redo",
  multiplayer: "Multiplayer and invite links",
  immediateFeedback: "Instant feedback delivery",
};

/** Labels of the features that are off, in a stable order (for the banner). */
export function unavailableFeatures(capabilities: Capabilities): string[] {
  return (Object.keys(LABELS) as (keyof Capabilities)[])
    .filter((key) => !capabilities[key])
    .map((key) => LABELS[key]);
}

export const CapabilityContext = createContext<Capabilities>(ONLINE_CAPABILITIES);

export const CapabilityProvider: React.FC<{ value: Capabilities; children: React.ReactNode }> = ({
  value,
  children,
}) => React.createElement(CapabilityContext.Provider, { value }, children);

export const useCapabilities = (): Capabilities => useContext(CapabilityContext);
export const useCapability = (name: keyof Capabilities): boolean => useCapabilities()[name];

/** Renders its children only when the capability is on. */
export const IfCapable: React.FC<{ name: keyof Capabilities; children: React.ReactNode }> = ({
  name,
  children,
}) => (useCapability(name) ? React.createElement(React.Fragment, null, children) : null);
