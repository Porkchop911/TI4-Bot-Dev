import React from "react";
import { useI48sSetting } from "../hooks/useI48sSetting.ts";

/** Under the I48S offer: switch the prompt off (it is then declined for you) or back on. */
export const I48sSwitch: React.FC = () => {
  const { ask, toggle } = useI48sSetting();
  return (
    <label
      data-testid="i48s-switch"
      style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, margin: "8px 0", cursor: "pointer" }}
    >
      <input type="checkbox" data-testid="i48s-switch-input" checked={!ask} onChange={toggle} />
      <span>Don't ask about I48S after each activation (decline it for me)</span>
    </label>
  );
};
