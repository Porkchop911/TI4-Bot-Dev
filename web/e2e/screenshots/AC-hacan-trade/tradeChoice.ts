// The transaction window a Sol seat sees opposite Hacan, with the option ids and payloads the engine
// builds (crates/ti4-engine/src/transactions.rs: offer_options and partner_assets).
export const tradeChoice = () => ({
  prompt: "transaction with hacan",
  context: { subtype: "propose_transaction", target: { Player: "other_seat" } } as Record<string, unknown>,
  options: [
    { id: "cc3", label: "swap 3 commodities each -- both gain", kind: "offer", payload: { net: 3, their_net: 3 } },
    { id: "ct3:2", label: "give 3 commodities for 2 trade goods", kind: "offer", payload: { net: -1, their_net: 1 } },
    { id: "pncf:sol:2", label: "sell cf:sol for 2 trade goods", kind: "offer", payload: { note: "cf:sol", alias: "cf", net: 2, their_net: -1 } },
    { id: "npconvoys:hacan:3", label: "pay 3 trade goods for the note convoys:hacan", kind: "offer", payload: { received_promissory: "convoys:hacan", net: 0, their_net: 3 } },
    { id: "cpconvoys:hacan:3", label: "pay 3 commodities for the note convoys:hacan", kind: "offer", payload: { received_promissory: "convoys:hacan", net: 0, their_net: 3 } },
    { id: "pcps:sol:2", label: "give the note ps:sol for 2 commodity", kind: "offer", payload: { promissory: "ps:sol", alias: "ps", net: 1, their_net: 0 } },
    { id: "nnps:sol>convoys:hacan", label: "give the note ps:sol for the note convoys:hacan", kind: "offer", payload: { promissory: "ps:sol", received_promissory: "convoys:hacan", net: 1, their_net: -1 } },
    { id: "cnrally>cf:hacan", label: "give the action card rally for the note cf:hacan", kind: "offer", payload: { action_card: "rally", promissory: "cf:hacan", net: 0, their_net: 1 } },
    { id: "decline", label: "Offer nothing", kind: "decline" },
  ],
});
