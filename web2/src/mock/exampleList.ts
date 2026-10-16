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
