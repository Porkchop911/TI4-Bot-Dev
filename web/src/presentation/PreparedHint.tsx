import React, { createContext, useContext } from "react";

/** The prepared answer for the decision on screen, so the usual UI can show it already picked. */
export interface PreparedHint {
  optionId: string;
  text: string;
  /** Warfare: the planned build batches (unit ids), staged in the real production builder. */
  builds?: string[];
}

const PreparedHintContext = createContext<PreparedHint | null>(null);

export const PreparedHintProvider: React.FC<{ value: PreparedHint | null; children: React.ReactNode }> = ({
  value,
  children,
}) => <PreparedHintContext.Provider value={value}>{children}</PreparedHintContext.Provider>;

/** `null` when nothing is prepared for the question being shown. */
export const usePreparedHint = (): PreparedHint | null => useContext(PreparedHintContext);
