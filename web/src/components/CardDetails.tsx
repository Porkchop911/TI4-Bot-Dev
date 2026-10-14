import React from "react";
import {
  getActionCardMeta,
  getPublicObjectiveMeta,
  getSecretObjectiveMeta,
  getStrategyCardMeta,
} from "../protocol/contentCatalog.ts";
import { DetailPanel } from "./DetailPanel.tsx";

export type CardSubject = {
  kind: "strategy" | "action" | "publicObjective" | "secretObjective";
  id: string;
};

export const CardDetails: React.FC<{ subject: CardSubject; onClose: () => void }> = ({
  subject,
  onClose,
}) => {
  if (subject.kind === "strategy") {
    const card = getStrategyCardMeta(subject.id);
    return (
      <DetailPanel title={card.name} onClose={onClose}>
        <div className="detail-panel__eyebrow">Initiative {card.initiative}</div>
        <h3>Primary ability</h3>
        <p>{card.primaryText || "No printed text available."}</p>
        <h3>Secondary ability</h3>
        <p>{card.secondaryText || "No printed text available."}</p>
      </DetailPanel>
    );
  }
  if (subject.kind === "action") {
    const card = getActionCardMeta(subject.id);
    return (
      <DetailPanel title={card.name} onClose={onClose}>
        <div className="detail-panel__eyebrow">Action card · {card.phase ?? "Action"}</div>
        <p>{card.description}</p>
      </DetailPanel>
    );
  }
  const card =
    subject.kind === "publicObjective"
      ? getPublicObjectiveMeta(subject.id)
      : getSecretObjectiveMeta(subject.id);
  return (
    <DetailPanel title={card.name} onClose={onClose}>
      <div className="detail-panel__eyebrow">
        {card.phase} · {card.points} VP
      </div>
      <h3>Requirement</h3>
      <p>{card.description}</p>
    </DetailPanel>
  );
};
