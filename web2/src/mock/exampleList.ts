// The examples of the demo, in the groups of the example list. This file has no imports: the
// demo bar and the screenshot tests both read it.
export const EXAMPLE_GROUPS: [string, [string, string][]][] = [
  [
    "Private draft",
    [
      ["draft-combat", "Draft · Space combat boundary"],
      ["draft-start", "Draft · Choose activation"],
      ["draft-movement", "Draft · Choose movement"],
      ["draft-movement-cases", "Draft · Movement, every edge case"],
      ["draft-rift", "Draft · Gravity rift boundary"],
      ["draft-invasion", "Draft · Choose landings"],
      ["draft-production", "Draft · Choose production"],
      ["draft-review", "Draft · Changed selection"],
      ["draft-cannon", "Draft · Space cannon boundary"],
    ],
  ],
  [
    "Game shell",
    [
      ["live-picker", "Live · Choose an action"],
      ["live-picker-4p", "Live · Choose an action (4 players)"],
      ["live-component", "Live · Component action"],
      ["live-waiting", "Live · Turn ended"],
      ["live-reaction", "Live · Reaction to another player's card"],
      ["live-decision-offer", "Live · Decision with answers"],
      ["live-decision-units", "Live · Decision: remove ships"],
    ],
  ],
  [
    "Strategy cards",
    [
      ["live-strategic", "Live · 1 Leadership (your primary)"],
      ["live-strategy-2", "Live · 2 Diplomacy (your primary)"],
      ["live-strategy-3", "Live · 3 Politics (your primary)"],
      ["live-strategy-4", "Live · 4 Construction (your primary)"],
      ["live-strategy-5", "Live · 5 Trade (your primary)"],
      ["live-strategy-6", "Live · 6 Warfare (your primary)"],
      ["live-strategy-7", "Live · 7 Technology (your primary)"],
      ["live-strategy-8", "Live · 8 Imperial (your primary)"],
      ["live-secondary", "Live · 1 Leadership (your secondary)"],
      ["live-secondary-2", "Live · 2 Diplomacy (your secondary)"],
      ["live-secondary-3", "Live · 3 Politics (your secondary)"],
      ["live-secondary-4", "Live · 4 Construction (your secondary)"],
      ["live-secondary-5", "Live · 5 Trade (your secondary)"],
      ["live-secondary-6", "Live · 6 Warfare (your secondary)"],
      ["live-secondary-7", "Live · 7 Technology (your secondary)"],
      ["live-secondary-8", "Live · 8 Imperial (your secondary)"],
    ],
  ],
  [
    "Live action",
    [
      ["live-combat", "Live · Space combat decisions"],
      ["live-combat-full", "Live · Space combat, every ship type"],
      ["live-invasion", "Live · Waiting in ground combat"],
      ["summary", "Live · Completed action"],
    ],
  ],
];
export const EXAMPLE_IDS = EXAMPLE_GROUPS.flatMap(([, items]) => items.map(([id]) => id));
