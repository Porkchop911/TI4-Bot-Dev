# Gameplay UI Completion Plan: Specification, UX Architecture & Work Packages

## 1. Overview & Context

This document is the definitive specification and implementation plan for the Twilight Imperium 4 browser gameplay UI, fulfilling the frontend scope of **Step 6** of [`web/plans/2026-09-21-ONLINE_MULTIPLAYER_ARCHITECTURE.md`](2026-09-21-ONLINE_MULTIPLAYER_ARCHITECTURE.md).

Per project instructions, this plan is strictly scoped to the **gameplay UI and browser client experience**. AI policy integration, neural network inference (`ti4-mlp`), and autonomous agent deciders are out of scope for this plan.

### 1.1 Current Baseline vs. Target State

| Dimension                 | Current Baseline (`Step 3B / Step 4`)                                             | Target Gameplay UI (`Step 6`)                                                                                                                                        |
| ------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Choice Representation** | Monolithic `PendingChoiceModal` radio-button list.                                | Domain-specific Choice Renderer Model dispatching to dedicated workflow surfaces (drawers, overlays, desks, banners).                                                |
| **Selection Constraints** | Ignores `min_selection` and `max_selection`; strictly single radio select.        | Fully enforces bounded multi-select (`min` to `max`), quantity allocation, and structured trade matrices.                                                            |
| **Payment Workflow**      | Sequential radio clicks per planet/trade good through repeated modal round-trips. | Interactive Economy Drawer with ready planet wallet, trade good stepper, live debt/credit tally, and pipelined execution.                                            |
| **Tactical Action**       | Selecting system hex IDs from a text list in a modal.                             | Direct SVG board click-to-activate, visual movement vector overlays, and a docked Fleet Rally Tray with cargo capacity gauges.                                       |
| **Combat Resolution**     | Sequential radio buttons (`destroy                                                | 0`, `sustain                                                                                                                                                         | 1`). | Dedicated Combat Arena with dice roll feed, staged sustain vs. direct hit vs. casualty windows, and retreat vector highlights. |
| **Bilateral Trade**       | Unparsed cryptic string IDs (`cc3`, `c3:0`, `pnsupport:sol`).                     | Structured Deal Catalog with categorized tabs (Commodity Swaps, Goods Exchange, Promissory Notes, Mutual Support), counter-offer builder, and accept/refuse actions. |
| **Agenda Voting**         | Flat list of outcomes followed by one-by-one planet exhaust radios.               | Full Agenda Council Board with live vote tallies per outcome, multi-planet influence basket, and Speaker tiebreaker controls.                                        |
| **Reaction Windows**      | Intrusive modal popup interrupting viewer for routine passes.                     | Non-blocking floating bottom Reaction Bar with countdown timer, "Fast Pass" shortcut, and pinned-favorite reaction settings.                                         |
| **Production**            | Isolated unit choices in modal.                                                   | Visual Production Cart tracking space dock capacity, resource costs, planet placement, and direct handoff to Payment Drawer.                                         |

---

## 2. Core Architectural Decisions

To ensure the plan is self-contained and ready for immediate execution, every open architectural and UX question is resolved here:

### Decision 1: Execution of Multi-Step Decisions via Semantic Pipeline Runner

- **Problem**: In `ti4-engine`, complex decisions like paying 5 resources across multiple planets or moving 6 ships into an active system are structured as synchronous loops of atomic choices (`Choice` with `pay X more` or `movement_step`). Furthermore, in [`crates/ti4-engine/src/tactical.rs:368`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-engine/src/tactical.rs#L368), ship movement option IDs are formatted as `{verb}|{origin}|{index}`, where `index` is the unit's position in `system.units`. When moving unit index 0, unit index 1 becomes index 0 in the next step. Raw static `option_id` matching fails across steps.
- **Decision**: **Semantic Pipeline Submission Engine (`usePipelineRunner.ts`)**.
  1. The user drafts their complete desired transaction locally in the UI (e.g., wallet, fleet rally, or casualty allocation).
  2. When the user clicks "Confirm Payment" or "Commit Fleet Move", the UI enters an executing state and launches the `PipelineSubmissionRunner`.
  3. Instead of static string comparison, the runner matches against **semantic intent predicates** using `option.payload`:
     - Movement: `{ origin: "24", unit: "cruiser" }` matches any newly offered option with identical origin and unit type, regardless of updated dynamic index.
     - Casualties: `{ unit: "fighter" }` matches any valid casualty option for that unit type.
     - Payment: `{ source: "planet_jord" }` or `{ kind: "trade_good" }`.
  4. Upon receiving each subsequent `pending_choice` from the server, the runner matches the next planned intent and submits the newly offered `option_id`.
  5. **Safety & Interruption Damping**: If an unexpected choice arrives (e.g., a Gravity Rift triggers a roll that destroys a ship, or an opponent opens a reaction window), the pipeline cleanly aborts, retains the unsubmitted remainder of the draft, and surfaces the intervening choice to the player.
  6. **Zero Engine Modifications**: Preserves 100% of engine determinism, timing windows, and authority without requiring composite batching endpoints on the server.

### Decision 2: Handling of Bounded Constraints (`min_selection` & `max_selection`)

- **Problem**: `PendingChoiceModal` currently renders radio buttons regardless of constraints. Some choices require selecting multiple items (e.g. discarding down to hand limit, choosing 2 technologies, selecting secret objectives during setup).
- **Decision**: **Bounded Multi-Select Control**.
  - When `constraints.max_selection > 1` or `constraints.min_selection > 1`:
    - The renderer dynamically switches from radio buttons to accessible checkbox cards (`role="checkbox"`, `aria-checked`).
    - The submit button is strictly disabled until `selected.size >= min_selection`.
    - Once `selected.size === max_selection`, unselected checkboxes are visually dimmed/disabled to prevent invalid selection states.
    - A clear badge displays: `Selected ${selected.size} of ${max_selection} required`.

### Decision 3: Board-First vs. Modal-First Spatial Interactions

- **Problem**: Should map targets (system activation, ship movement origin/destinations, planet landings) be picked inside a dialog or directly on the map?
- **Decision**: **Hybrid Board-First Interaction**.
  - **Spatial Actions** (System Activation, Fleet Rally, Invasion, Bombardment): The choice modal auto-minimizes into a compact status prompt ("Select a system to activate"). The SVG galaxy board highlights legal candidate hexes with animated target reticles. Clicking a hex or planet directly selects that target and executes or stages the move.
  - **Resource / Inventory Actions** (Payment, Production): Modeless sliding drawer from the right/bottom, keeping the map visible underneath.
  - **High-Stakes Focal Events** (Combat, Bilateral Trade, Agenda Council): Focused, accessible modal dialog with backdrop blur and explicit "Inspect Board" minimization toggle.

### Decision 4: Reaction Windows & Pinned Auto-Pass Strategy

- **Problem**: In `ti4-engine` ([`crates/ti4-engine/src/reactions.rs:768`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-engine/src/reactions.rs#L768)), the engine already silently skips seats with no playable cards. If a choice reaches the player, they definitely hold an eligible reaction card or ability. However, popping up a modal for every opportunity slows down play.
- **Decision**: **Non-Blocking Reaction Bar with Pinned Fast-Pass**.
  - Optional reactions (`context.optional === true` and `subtype.startsWith('play_reaction_')`) render as an unobtrusive bottom floating pill (`ReactionStatusBar.tsx`) rather than a modal dialog.
  - **Pinned Settings**: Players can configure `"Auto-pass reaction windows unless pinned/favorited"` (e.g. pinning only _Sabotage_ and _Direct Hit_).
  - **Fast-Pass**: Pressing `Spacebar` or clicking "Pass" immediately submits `decline`.
  - A visual countdown bar (default 10s for active timers in multiplayer) informs the table of the window remaining.

### Decision 5: Bilateral Trade: Structured Deal Catalog vs. Composite Bundles

- **Problem**: In [`crates/ti4-engine/src/transactions.rs:825-865`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-engine/src/transactions.rs#L825-L865), `ti4-engine` only generates discrete atomic transaction shapes (`cc{n}`, `ct{g}:{w}`, `tc{g}:{w}`, `{g}:{w}`, `c{n}:0`, `pn{note}:{price}`, `ss`). An unconstrained freeform basket builder would allow assembling arbitrary multi-item bundles that have no corresponding legal `option_id` in the engine.
- **Decision**: **Structured Deal Catalog**.
  - The Bilateral Trade Desk is organized into categorized tabs matching the engine's legal offer shapes:
    1. **Commodity Swaps**: Swap equal commodities (`cc{n}`).
    2. **Goods & Commodities Exchange**: Asymmetric swaps of commodities for trade goods (`ct{g}:{w}` / `tc{g}:{w}`).
    3. **Promissory Notes**: Buy, sell, or gift specific promissory notes (`pn{note}:{price}`).
    4. **Relic Fragments & Secret Objectives**: Trade fragments or unscored secret objectives under _Black Market Dealings_ (`fr{trait}:{price}`, `so{secret}:{price}`).
    5. **Mutual Support**: Bilateral Support for the Throne swap (`ss`).
  - The UI controls configure parameters within these discrete legal shapes, directly mapping the chosen tab and values to an offered engine `option_id`.

### Decision 6: Authoritative Dice Presentation & Animation

- **Problem**: The engine rolls all dice server-side using seeded ChaCha8 RNG. How should this be displayed to players without creating desynchronization?
- **Decision**: **Authoritative Event-Driven Dice Animation**.
  - The server records rolls in `GameEvent` logs (`CombatRoll { player, unit, roll, target, hit }`).
  - The client Combat Arena plays a brief (400ms) roll animation for the dice, landing on the authoritative values from the event log.
  - An "Instant / Skip Animation" toggle is provided for fast play.

### Decision 7: Multi-Stage Combat Phasing & Direct Hit Protection

- **Problem**: Grouping Sustain Damage and Casualty Destruction into a single simultaneous form fails because in TI4 rules and `ti4-engine`, they are strictly sequential phases separated by opponent reaction windows:
  1. `sustain_damage`: Player chooses whether to sustain hits on Dreadnoughts/Warsuns.
  2. If sustained, opponents get an opportunity to play **Direct Hit** (destroying that ship).
  3. Only after all sustains and reactions finish does the engine transition to `assign_casualty` for remaining uncancelled hits.
- **Decision**: **Sequential Multi-Stage Combat Arena**.
  - The Combat Arena progresses through three distinct visual stages:
    - **Stage 1 (Sustain Damage Window)**: Interactive unit cards display "Sustain Damage" toggles.
    - **Stage 2 (Opponent Reaction Pause)**: Displays a waiting banner: "Awaiting opponent reaction window (Direct Hit)...".
    - **Stage 3 (Casualty Allocation Window)**: Displays remaining uncancelled hits with unit destruction counters.
  - Sustained ships show a yellow caution badge indicating pending status until Stage 2 resolves.

### Decision 8: Opponent & Spectator Visibility During Focal Events

- **Problem**: What do non-active players and spectators see during tactical movement, combat, and trade?
- **Decision**: **Explicit Redacted Observer Roles**.
  - **Combat**: Spectators and non-combatant opponents see the Combat Arena in read-only **Spectator Mode**, displaying live dice rolls, battle participants, round numbers, and casualty counts.
  - **Tactical Action**: All players see the active system highlighted with an animated gold ring and an "Active System" banner. Movement vectors are visible once committed to the engine.
  - **Bilateral Trade**: Negotiation terms remain strictly private between the two negotiating seats; opponents see a public turn status: `"Player A negotiating transaction with Player B"`.

### Decision 9: Streamlined Multi-Casualty Steppers

- **Problem**: In large fleet battles where a player loses 8 fighters, clicking 8 individual checkboxes or radio buttons is tedious and error-prone.
- **Decision**: **Unit Quantity Steppers & Auto-Allocation**.
  - Identical units in `assign_casualty` are grouped into steppers: `Fighter [ - 4 + ] (Available: 6)`.
  - Quick action buttons: `[ Max ]`, `[ Clear ]`, `[ Auto-assign Cheapest ]`.
  - The semantic pipeline runner expands `count: 4` into 4 sequential `assign_casualty` submissions.

---

## 3. Choice Renderer Model Specification

### 3.1 Domain Types (`web/src/presentation/choiceModel.ts`)

```typescript
import {
  PendingChoiceDto,
  ChoiceOptionDto,
  DecisionContextDto,
  OutstandingConstraintDto,
  DecisionTargetDto,
} from "../protocol/types.ts";

export type SelectionMode =
  | { mode: "single" } // 1-of-N (Radio, click card, or click hex)
  | { mode: "multi"; min: number; max: number } // Bounded multi-select (Checkboxes, multi-cards)
  | { mode: "quantity"; target: number; paid: number; unit: "resources" | "influence" | "votes" }
  | { mode: "transaction"; partnerSeat: string } // Structured deal catalog
  | { mode: "tactical_move"; activeSystem: string } // Multi-origin ship rally
  | { mode: "tactical_cargo"; activeSystem: string } // Fighter and infantry cargo loading
  | { mode: "tactical_invasion"; activeSystem: string } // Ground commitment to planets
  | { mode: "production"; capacity: number; systemId: string } // Unit build cart
  | { mode: "sustain"; hitsRemaining: number } // Sustain damage allocation
  | { mode: "casualty"; hitsToAssign: number }; // Unit destruction allocation

export type ChoiceWorkflowKind =
  | "system_activation" // Activating a system on the board
  | "tactical_movement" // Moving ships into active system
  | "tactical_cargo" // Loading cargo into carriers / dreadnoughts
  | "tactical_invasion" // Committing ground forces to planets
  | "payment" // Paying resources or influence (planets + trade goods)
  | "production" // Building units at production structures
  | "combat_sustain" // Sustaining damage on capital ships
  | "combat_casualty" // Destroying units to satisfy uncancelled hits
  | "combat_retreat" // Announcing or choosing retreat destination
  | "agenda_vote_outcome" // Selecting outcome or abstaining
  | "agenda_vote_planets" // Exhausting planets / spending influence for votes
  | "transaction_propose" // Building an offer for another player
  | "transaction_answer" // Accepting, countering, or refusing an offer
  | "action_card_reaction" // Fast reaction window (Sabotage, timing triggers)
  | "objective_scoring" // Public or secret objective fulfillment
  | "generic_selection"; // Fallback list / modal

export interface ChoiceRendererModel {
  readonly workflow: ChoiceWorkflowKind;
  readonly selectionMode: SelectionMode;
  readonly prompt: string;
  readonly actor: string;
  readonly nonce: string;
  readonly isOptional: boolean;
  readonly contextTarget: DecisionTargetDto | null;
  readonly options: readonly ChoiceOptionDto[];
  readonly outstanding: readonly OutstandingConstraintDto[];

  // Categorized options
  readonly declineOption: ChoiceOptionDto | null;
  readonly optionsByKind: ReadonlyMap<string, ChoiceOptionDto[]>;
  readonly optionsByTarget: ReadonlyMap<string, ChoiceOptionDto[]>;
}

// Typed payload accessors (Zero raw string parsing)
export function getPaymentPayload(opt: ChoiceOptionDto): {
  worth: number;
  owed: number;
  kind: "resources" | "influence";
  source?: string;
} {
  const p = opt.payload ?? {};
  return {
    worth: Number(p.worth ?? 0),
    owed: Number(p.owed ?? 0),
    kind: String(p.kind ?? "resources").toLowerCase() === "influence" ? "influence" : "resources",
    source: typeof p.source === "string" ? p.source : undefined,
  };
}

export function getMovementPayload(opt: ChoiceOptionDto): {
  origin?: string;
  unit?: string;
  damaged?: boolean;
  capacity?: number;
  gravity_drive?: boolean;
} {
  const p = opt.payload ?? {};
  return {
    origin: typeof p.origin === "string" ? p.origin : undefined,
    unit: typeof p.unit === "string" ? p.unit : undefined,
    damaged: Boolean(p.damaged),
    capacity: typeof p.capacity === "number" ? p.capacity : undefined,
    gravity_drive: Boolean(p.gravity_drive),
  };
}

export function getTradePayload(opt: ChoiceOptionDto): {
  net?: number;
  their_net?: number;
} {
  const p = opt.payload ?? {};
  return {
    net: typeof p.net === "number" ? p.net : undefined,
    their_net: typeof p.their_net === "number" ? p.their_net : undefined,
  };
}
```

### 3.2 Authoritative Classification Function

This function aligns 1:1 with authoritative engine subtypes in `crates/ti4-engine`:

```typescript
export function deriveChoiceRendererModel(
  choice: PendingChoiceDto | null,
  viewerSeat: string | null,
): ChoiceRendererModel | null {
  if (!choice || !viewerSeat || choice.actor !== viewerSeat) {
    return null;
  }

  const subtype = choice.context?.subtype ?? "";
  const isOptional = Boolean(
    choice.context?.optional || choice.options.some((o) => o.id === "decline"),
  );
  const declineOption =
    choice.options.find((o) => o.id === "decline" || o.kind === "decline") ?? null;
  const constraints = choice.constraints ?? choice.context?.outstanding?.[0];

  // Grouping
  const optionsByKind = new Map<string, ChoiceOptionDto[]>();
  for (const opt of choice.options) {
    const list = optionsByKind.get(opt.kind ?? "default") || [];
    list.push(opt);
    optionsByKind.set(opt.kind ?? "default", list);
  }

  // 1. Payment & Economy (Exact engine subtypes from production.rs and choice.rs)
  if (
    subtype === "pay_resources" ||
    subtype === "pay_influence" ||
    subtype === "spend_command_tokens" ||
    subtype === "leadership_spend_influence" ||
    subtype === "tactical_production" ||
    optionsByKind.has("pay")
  ) {
    const owed = constraints?.amount ?? 0;
    const paid = constraints?.paid ?? 0;
    const kind =
      subtype === "pay_influence" || constraints?.kind?.toLowerCase() === "influence"
        ? "influence"
        : "resources";
    return {
      workflow: "payment",
      selectionMode: { mode: "quantity", target: owed, paid, unit: kind },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 2. System Activation
  if (
    subtype === "activate_system" ||
    (choice.options.length > 0 && choice.options.every((o) => o.kind === "activate"))
  ) {
    return {
      workflow: "system_activation",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: false,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption: null,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 3. Tactical Movement, Cargo, and Invasion Commit
  if (subtype === "movement_step" || choice.prompt.toLowerCase().includes("movement")) {
    const activeSystem =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    return {
      workflow: "tactical_movement",
      selectionMode: { mode: "tactical_move", activeSystem },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  if (subtype === "load_cargo") {
    const activeSystem =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    return {
      workflow: "tactical_cargo",
      selectionMode: { mode: "tactical_cargo", activeSystem },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  if (subtype === "commit_ground_forces") {
    const activeSystem =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    return {
      workflow: "tactical_invasion",
      selectionMode: { mode: "tactical_invasion", activeSystem },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // Production Builder (production.rs)
  if (subtype === "produce_unit" || subtype === "place_unit") {
    const systemId =
      choice.context?.target && "System" in choice.context.target
        ? choice.context.target.System
        : "";
    const capacity = constraints?.amount ?? 0;
    return {
      workflow: "production",
      selectionMode: { mode: "production", capacity, systemId },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: Boolean(declineOption),
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 4. Combat Phasing: Sustain vs Casualties vs Retreat
  if (subtype === "sustain_damage") {
    return {
      workflow: "combat_sustain",
      selectionMode: { mode: "sustain", hitsRemaining: constraints?.amount ?? 1 },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  if (subtype === "assign_casualty" || optionsByKind.has("casualty")) {
    const hits = constraints?.amount ?? 1;
    return {
      workflow: "combat_casualty",
      selectionMode: { mode: "casualty", hitsToAssign: hits },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: false,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption: null,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  if (subtype === "announce_retreat" || subtype === "retreat_to" || optionsByKind.has("retreat")) {
    return {
      workflow: "combat_retreat",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 5. Bilateral Transactions (Structured Deal Catalog)
  if (
    subtype === "propose_transaction" ||
    subtype === "answer_transaction" ||
    optionsByKind.has("offer")
  ) {
    const partnerSeat =
      choice.context?.target && "Player" in choice.context.target
        ? choice.context.target.Player
        : "";
    return {
      workflow: subtype === "answer_transaction" ? "transaction_answer" : "transaction_propose",
      selectionMode: { mode: "transaction", partnerSeat },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 6. Agenda Voting
  if (subtype === "cast_vote" || subtype === "vote_exhaust_planet" || subtype === "vote_tiebreak") {
    return {
      workflow: subtype === "vote_exhaust_planet" ? "agenda_vote_planets" : "agenda_vote_outcome",
      selectionMode:
        subtype === "vote_exhaust_planet"
          ? { mode: "quantity", target: 0, paid: 0, unit: "votes" }
          : { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: choice.context?.outstanding ?? [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 7. Optional Reactions (Exact engine pattern `play_reaction_*`)
  if (
    subtype.startsWith("play_reaction_") ||
    (isOptional &&
      choice.options.length <= 4 &&
      choice.context?.source &&
      "Reaction" in choice.context.source)
  ) {
    return {
      workflow: "action_card_reaction",
      selectionMode: { mode: "single" },
      prompt: choice.prompt,
      actor: choice.actor,
      nonce: choice.nonce,
      isOptional: true,
      contextTarget: choice.context?.target ?? null,
      options: choice.options,
      outstanding: [],
      declineOption,
      optionsByKind,
      optionsByTarget: new Map(),
    };
  }

  // 8. Bounded Multi-Selection Fallback
  const min = constraints?.min_selection ?? 1;
  const max =
    constraints?.max_selection ?? (constraints?.min_selection ? constraints.min_selection : 1);
  return {
    workflow: "generic_selection",
    selectionMode: max > 1 ? { mode: "multi", min, max } : { mode: "single" },
    prompt: choice.prompt,
    actor: choice.actor,
    nonce: choice.nonce,
    isOptional,
    contextTarget: choice.context?.target ?? null,
    options: choice.options,
    outstanding: choice.context?.outstanding ?? [],
    declineOption,
    optionsByKind,
    optionsByTarget: new Map(),
  };
}
```

---

## 4. Workflow UX Flows & Component Architecture

### 4.1 Payment & Economy Drawer (`PaymentDrawer.tsx`)

#### UX Flow Diagram

```
[Engine issues: pay_resources or pay_influence]
                    │
                    ▼
[Drawer slides open from right (board stays visible)]
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│ Player Wallet Display:                                  │
│ • Ready Planet Cards: Jord [R:4 I:2] (Toggle)          │
│ • Ready Planet Cards: Arinam [R:1 I:2] (Toggle)         │
│ • Trade Goods Counter: [ - 2 + ] (1 TG = 1 Res/Inf)     │
│ • Active Modifiers: Sarween Tools (-1 Res)              │
├─────────────────────────────────────────────────────────┤
│ Live Tally Bar:                                         │
│ [=======================>      ] 4 / 6 Paid (Credit: 0) │
└─────────────────────────────────────────────────────────┘
                    │
                    ▼
[Player clicks planets / adjusts TG until Committed >= Cost]
                    │
                    ▼
[Click "Confirm Payment ($6)"]
                    │
                    ▼
[PipelineRunner auto-submits drafted options sequentially]
                    │
                    ▼
[Debt settled -> Drawer closes automatically]
```

#### Detailed Interactions & Edge Cases

- **Archon's Gift / Dual-Value Planets**: Planets with both resource and influence values display their spendable face prominently based on the required currency. If an ability permits cross-spending, both faces are shown with a selector.
- **Overpayment & Credit**: Overpaying (e.g. using a 4-resource planet for a 3-resource token) displays an informational note: `1 Resource will be retained as credit for the remainder of this transaction`.
- **Keyboard Shortcuts**: Numbers `1-9` toggle the first 9 planets; `+` / `-` increment/decrement trade goods; `Enter` confirms payment.

---

### 4.2 Tactical Movement Suite (`TacticalMovementOverlay.tsx`)

#### UX Flow Diagram

```
[Tactical Action: Active System Selected on Board]
                       │
                       ▼
[Map displays green movement vectors from neighboring systems]
                       │
                       ▼
[Player clicks an Origin System Hex on the Map]
                       │
                       ▼
┌───────────────────────────────────────────────────────────────┐
│ Fleet Rally Tray (Docked at Board Bottom):                    │
│ • Origin: System #24 (Moll Primus)                            │
│ • Ships to Move:                                              │
│   - Carrier [ - 1 + ] (Cap: 4)                                │
│   - Cruiser [ - 2 + ]                                         │
│   - Fighter [ - 3 + ] (Cargo)                                 │
│   - Infantry [ - 1 + ] (Cargo)                                │
├───────────────────────────────────────────────────────────────┤
│ Gauges:                                                       │
│ • Fleet Supply: 3 / 4 Ships (OK)                              │
│ • Cargo Capacity: 4 / 4 Loaded (OK)                           │
└───────────────────────────────────────────────────────────────┘
                       │
                       ▼
[Staged ships appear as translucent ghost units in active system]
                       │
                       ▼
[Player clicks "Commit Fleet Moves" or "Finish Movement"]
                       │
                       ▼
[Semantic PipelineRunner submits moves matching `{ origin, unit }`]
                       │
                       ▼
[Transitions seamlessly into `load_cargo` or `commit_ground_forces`]
```

#### Detailed Interactions & Edge Cases

- **Semantic Intent Matching**: Moving multiple Cruisers from system 24 matches against payload `{ origin: "24", unit: "cruiser" }`, unaffected by dynamic unit index shifts.
- **Gravity Drive Selection**: If the player has Gravity Drive, ship rows display an optional `[ +1 Move ]` checkbox badge. Selecting it binds the `gravity_drive` payload parameter.
- **Cargo & Ground Commitment**: After ships arrive, `load_cargo` and `commit_ground_forces` use the same docked tray layout, avoiding abrupt modal switches.

---

### 4.3 Combat Resolution Arena (`CombatResolutionModal.tsx`)

#### UX Flow Diagram

```
[Combat Phase Triggered in System #18]
                    │
                    ▼
[Combat Arena Opens (Full Dialog with Board Backdrop)]
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│ Stage Header: SPACE COMBAT — Round 1                    │
│ Attacker: Player 1 (Sol) vs. Defender: Player 2 (Letnev)│
├─────────────────────────────────────────────────────────┤
│ Dice Results Feed:                                      │
│ • 2x Dreadnought (5+): [ 7 ] [ 4 ] -> 1 HIT             │
│ • 1x Cruiser (7+):     [ 8 ]       -> 1 HIT             │
│ • 4x Fighter (9+):     [ 3 ][ 9 ]  -> 1 HIT             │
│ TOTAL HITS TO ALLOCATE: 3                               │
├─────────────────────────────────────────────────────────┤
│ STAGE 1: Sustain Damage Window (`sustain_damage`)        │
│ [ Dreadnought ] -> [ Sustain Damage ] (Checkbox)        │
│ "Caution: Opponents may react with Direct Hit"          │
└─────────────────────────────────────────────────────────┘
                    │ (Confirm Sustains)
                    ▼
┌─────────────────────────────────────────────────────────┐
│ STAGE 2: Opponent Reaction Pause                        │
│ "Awaiting opponent reaction window..."                  │
└─────────────────────────────────────────────────────────┘
                    │ (Reactions Clear)
                    ▼
┌─────────────────────────────────────────────────────────┐
│ STAGE 3: Casualty Allocation Window (`assign_casualty`) │
│ Units to Destroy:                                       │
│ • Cruiser  [ - 1 + ] (Available: 1)                     │
│ • Fighter  [ - 2 + ] (Available: 4)  [ Auto-Cheapest ]  │
│ Allocated: 3 / 3 Hits                                   │
└─────────────────────────────────────────────────────────┘
                    │
                    ▼
[Click "Confirm Casualties" -> Transitions to next combat round]
```

#### Detailed Interactions & Edge Cases

- **Sequential Phasing**: Sustain Damage and Casualty Allocation are strictly decoupled to respect TI4 Direct Hit reaction windows.
- **Grouped Steppers**: Identical units (e.g. 6 Fighters) are grouped into steppers with quick `[ Max ]`, `[ Clear ]`, and `[ Auto-Cheapest ]` helpers.
- **Spectator Mode**: Non-combatants view the Combat Arena in read-only mode showing live dice feeds and unit casualties without action buttons.

---

### 4.4 Bilateral Trade Desk (`TradeDeskModal.tsx`)

#### Structured Deal Catalog Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Bilateral Trade Desk: You (Sol) <---> Partner (Hacan)                       │
├─────────────────────────────────────────────────────────────────────────────┤
│ [ Commodities ]  [ Goods Exchange ]  [ Promissory Notes ]  [ Mutual Support ]│
├─────────────────────────────────────────────────────────────────────────────┤
│ TAB: Promissory Notes (`pn{note}:{price}`)                                  │
│ • Select Note to Offer:                                                     │
│   [x] Sol Promissory: Military Support                                      │
│ • Asking Price (Trade Goods from Partner):                                  │
│   [ - 3 + ] Trade Goods (Partner holds: 5)                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ Authoritative Legal Option: `pnmilitary_support:sol:3`                      │
│ Net Value: +3 Trade Goods                                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│ Actions: [ Propose Deal ]                              [ Decline / Close ]  │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Detailed Interactions & Edge Cases

- **Catalog Alignment**: Tabs strictly correspond to legal engine shapes:
  - Commodity Swaps (`cc{n}`)
  - Goods Exchange (`ct{g}:{w}`, `tc{g}:{w}`, `{g}:{w}`, `c{n}:0`)
  - Promissory Notes (`pn{note}:{price}`)
  - Mutual Support (`ss`)
- **Partner Answering Mode**: Partner sees the inbound offer with clear `Accept Deal`, `Counter-Offer`, and `Refuse` actions. Counter-offer keeps existing terms on the desk and switches into proposer edit mode.

---

### 4.5 Agenda Voting & Council Board (`AgendaBallotModal.tsx`)

#### UX Flow Diagram

```
[Agenda Phase: Agenda Card Revealed: "Fleet Regulations"]
                          │
                          ▼
┌───────────────────────────────────────────────────────────┐
│ Imperial Council Ballot: "Fleet Regulations" (Directive)  │
│ Description: Each player cannot have more than 4 ships... │
├───────────────────────────────────────────────────────────┤
│ Outcomes & Live Standings:                                │
│ • FOR:     12 Votes (Seats 1, 3)                          │
│ • AGAINST: 8 Votes  (Seat 2)                              │
│ • ABSTAIN: 0 Votes                                        │
├───────────────────────────────────────────────────────────┤
│ Your Turn to Vote (Seat 4 - Yssaril):                     │
│ Step 1: Select Outcome: [ FOR ]  [ AGAINST ]  [ ABSTAIN ] │
│ Step 2: Commit Votes:                                     │
│   - Exhaust Planet: Mecatol Rex (6 Influence) [x]         │
│   - Exhaust Planet: Retillion   (3 Influence) [x]         │
│   - Spend Trade Goods: [ - 0 + ]                          │
│ TOTAL VOTES TO CAST: 9                                    │
└───────────────────────────────────────────────────────────┘
                          │
                          ▼
[Click "Cast 9 Votes FOR" -> Pipelined submission resolves votes]
```

#### Detailed Interactions & Edge Cases

- **Speaker Tiebreaker**: If the final tally results in a tie, a special gavel animation appears for the Speaker seat: "Break the Tie" with outcome buttons.
- **Riders & Prediction**: Players who played Agenda Riders (e.g. Leadership Rider) see an icon next to their chosen outcome indicating their active stake.

---

### 4.6 Unobtrusive Reaction Window Bar (`ReactionStatusBar.tsx`)

#### Component Layout

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ⚡ REACTION WINDOW: Sabotage Opportunity               [ 0:08 ] [ ⏸ Pause ]│
│ Active Player played: "Morale Boost" in Combat System #18                 │
│ Action: [ Play Sabotage (Hand: 1) ]        [ Fast Pass (Spacebar) ]         │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Behavioral Rules

- **Non-Modal Injection**: Mounted directly into `GameShell` bottom overlay stack without creating a dialog backdrop.
- **Keyboard Navigation**: Pressing `Spacebar` passes immediately (`decline`). Pressing `Enter` triggers the primary reaction card.
- **Pinned Settings**: Option in user preferences: `"Auto-pass reaction windows unless pinned/favorited"`.

---

## 5. State Management & Protocol Pipeline

### 5.1 Semantic Pipeline Submission Runner (`web/src/hooks/usePipelineRunner.ts`)

```typescript
export interface SemanticIntent {
  kind: "movement" | "casualty" | "payment";
  predicate: (option: ChoiceOptionDto) => boolean;
  description: string;
}

export function usePipelineRunner(
  pendingChoice: PendingChoiceDto | null,
  submitChoice: (optionId: string) => Promise<void>,
) {
  const [activeQueue, setActiveQueue] = useState<SemanticIntent[]>([]);
  const [isRunning, setIsRunning] = useState(false);

  useEffect(() => {
    if (!isRunning || activeQueue.length === 0 || !pendingChoice) return;

    const nextIntent = activeQueue[0];
    const matchingOption = pendingChoice.options.find(nextIntent.predicate);

    if (matchingOption) {
      submitChoice(matchingOption.id)
        .then(() => {
          setActiveQueue((prev) => prev.slice(1));
        })
        .catch(() => {
          setIsRunning(false);
          setActiveQueue([]);
        });
    } else {
      // Intervening decision occurred (e.g. Gravity Rift roll or Reaction window)
      // Pause pipeline and surface the intervening decision
      setIsRunning(false);
      setActiveQueue([]);
    }
  }, [pendingChoice?.nonce, isRunning]);

  const executePipeline = (intents: SemanticIntent[]) => {
    if (intents.length === 0) return;
    setActiveQueue(intents);
    setIsRunning(true);
  };

  return { executePipeline, isRunning, queueLength: activeQueue.length };
}
```

---

## 6. Complete Work Packages (UI-01 through UI-08)

To keep implementation risk low and enforce modular verification, packages `UI-04` and `UI-06` are split into atomic subpackages:

```
[UI-01: Choice Model & Classifier]
          │
          ▼
[UI-02: Bounded Multi-Selection & Dynamic Option List]
          │
          ├───────────────────────────────┬───────────────────────────────┐
          ▼                               ▼                               ▼
[UI-03: Economy & Payment Drawer]  [UI-04a: Activation & Board Vectors]  [UI-05: Combat Arena]
          │                               │                               │
          │                        [UI-04b: Fleet Tray & Semantic Runner] │
          │                               │                               │
          └───────────────────────────────┼───────────────────────────────┘
                                          ▼
                         [UI-06a: Bilateral Trade Catalog Desk]
                                          │
                         [UI-06b: Agenda Council Ballot Desk]
                                          │
                                          ▼
                         [UI-07: Reaction Bar & Production Cart]
                                          │
                                          ▼
                         [UI-08: Shell Assembly & E2E Validation]
```

---

### Package Specifications

#### UI-01: Choice Model Classification & Presentation Refactor

- **Objective**: Implement the domain classification engine matching exact engine subtypes.
- **Writable Paths**: `web/src/presentation/choiceModel.ts`, `web/src/presentation/choiceModel.test.ts`.
- **Deliverables**:
  - Implement `deriveChoiceRendererModel()` covering all engine subtypes (`pay_resources`, `pay_influence`, `movement_step`, `load_cargo`, `commit_ground_forces`, `produce_unit`, `place_unit`, `sustain_damage`, `assign_casualty`, `play_reaction_*`).
  - Implement typed payload accessors (`getPaymentPayload`, `getMovementPayload`, `getTradePayload`).
  - Unit test suite asserting classification for every game phase and decision subtype.
- **Verification**: `npm test src/presentation/choiceModel.test.ts`.

#### UI-02: Bounded Multi-Selection & Dynamic Option List

- **Objective**: Replace radio-only inputs in `PendingChoiceModal` with dynamic single/multi selection controls.
- **Writable Paths**: `web/src/components/PendingChoiceModal.tsx`, `web/src/components/PendingChoiceModal.test.tsx`.
- **Deliverables**:
  - Inspect `constraints.min_selection` and `max_selection`.
  - Checkbox multi-select mode with selection counters and boundary enforcement.
  - Search/filter bar for large option lists (e.g. Tech trees, Action cards).
  - Keyboard focus trap and ARIA checkbox accessibility.
- **Verification**: `npm test src/components/PendingChoiceModal.test.tsx`.

#### UI-03: Dedicated Payment & Economy Drawer

- **Objective**: Build the visual wallet and pipelined debt settlement interface.
- **Writable Paths**: `web/src/components/PaymentDrawer.tsx`, `web/src/components/PaymentDrawer.test.tsx`, `web/src/hooks/usePipelineRunner.ts`.
- **Deliverables**:
  - Interactive planet cards displaying resources/influence values with toggleable exhaust states.
  - Trade good / commodity stepper controls.
  - Real-time `Paid / Owed / Credit` progress meter.
  - Pipelined sequential submission runner executing multi-planet payments.
- **Verification**: Vitest tests simulating resource payment, overpayment credit, and trade good spending.

#### UI-04a: System Activation & Board Vector Overlays

- **Objective**: Build SVG board target reticles and movement vector overlays for tactical actions.
- **Writable Paths**: `web/src/components/Board.tsx`, `web/src/presentation/boardPresentation.ts`.
- **Deliverables**:
  - Target reticles and pulse rings on candidate systems during `activate_system`.
  - Animated vector lines from eligible origin systems to active system during `movement_step`.
  - Direct board click-to-activate and click-to-rally handlers.
- **Verification**: `npm test src/presentation/boardPresentation.test.ts`.

#### UI-04b: Fleet Rally Tray, Cargo Gauges & Semantic Runner

- **Objective**: Build the docked fleet movement tray and semantic pipeline runner.
- **Writable Paths**: `web/src/components/TacticalMovementOverlay.tsx`, `web/src/hooks/usePipelineRunner.ts`.
- **Deliverables**:
  - Docked Fleet Rally Tray with ship quantity steppers.
  - Real-time Fleet Supply and Cargo Capacity meters.
  - Ghost unit staging in active system before commit.
  - Semantic intent runner matching `{ origin, unit }` across dynamic unit index shifts.
- **Verification**: Component tests for fleet limits and cargo capacity verification.

#### UI-05: Multi-Stage Combat Arena & Casualty Steppers

- **Objective**: Build visual combat staging, dice roll feed, and casualty allocation.
- **Writable Paths**: `web/src/components/CombatResolutionModal.tsx`, `web/src/components/CombatResolutionModal.test.tsx`.
- **Deliverables**:
  - Three-stage progression: `sustain_damage` window -> opponent reaction pause -> `assign_casualty` window.
  - Dice results feed displaying unit roll targets and hit indicators.
  - Grouped unit casualty steppers (`Fighter [ - 4 + ]`) with `Max`, `Clear`, and `Auto-Cheapest`.
  - Spectator read-only mode for non-combatant seats.
- **Verification**: Vitest tests validating sustain toggles, casualty counts, and direct-hit warnings.

#### UI-06a: Bilateral Trade Desk & Structured Deal Catalog

- **Objective**: Build the structured trade desk matching discrete engine transaction shapes.
- **Writable Paths**: `web/src/components/TradeDeskModal.tsx`, `web/src/presentation/tradeDecoder.ts`.
- **Deliverables**:
  - Categorized catalog tabs: Commodity Swaps (`cc{n}`), Goods Exchange (`ct{g}:{w}`), Promissory Notes (`pn{note}:{price}`), Mutual Support (`ss`).
  - Partner inbound offer desk with `Accept Deal`, `Counter-Offer`, and `Refuse` actions.
- **Verification**: Vitest tests for legal option mapping and trade value delta calculation.

#### UI-06b: Agenda Council Ballot Desk & Speaker Gavel

- **Objective**: Build the imperial council voting board and Speaker tiebreaker.
- **Writable Paths**: `web/src/components/AgendaBallotModal.tsx`.
- **Deliverables**:
  - Live outcome tallies and seat-by-seat voting order.
  - Multi-planet influence commitment basket.
  - Speaker tiebreaker gavel view for `vote_tiebreak`.
- **Verification**: Vitest tests for agenda ballot tally verification.

#### UI-07: Unobtrusive Reaction Window Bar & Production Cart

- **Objective**: Implement modeless reaction HUD and space dock unit production cart.
- **Writable Paths**: `web/src/components/ReactionStatusBar.tsx`, `web/src/components/ProductionBuilderDrawer.tsx`.
- **Deliverables**:
  - Floating bottom reaction bar with countdown timer and `Spacebar` fast-pass.
  - Pinned reaction preferences (auto-pass unpinned windows).
  - Production cart tracking production capacity, resource costs, and destination planets.
- **Verification**: Component tests for fast-pass keyboard events and production limit enforcement.

#### UI-08: Shell Assembly, Responsive Polish & E2E Verification

- **Objective**: Unify all workflow renderers into `GameShell`, ensure responsive layouts, and validate with multi-seat Playwright tests.
- **Writable Paths**: `web/src/components/GameShell.tsx`, `web/src/App.tsx`, `web/e2e/gameplay_workflows.spec.ts`.
- **Deliverables**:
  - Top-level `ChoiceRendererDispatcher` routing choices to the appropriate surface.
  - Responsive layout adaptations for mobile (<768px), tablet (768-1279px), and desktop (>=1280px).
  - Playwright end-to-end multi-seat test covering:
    1. Tactical action (activation -> fleet move -> combat -> hit allocation -> invasion -> production -> payment).
    2. Bilateral trade negotiation (proposal -> counter-offer -> acceptance).
    3. Agenda voting with planet exhaustion.
- **Verification**: Full test suite: `npm test` and `npx playwright test`.

---

## 7. Accessibility & Ergonomics Standards

### WCAG 2.1 AA Compliance

- **Keyboard Navigation**:
  - `Tab` / `Shift+Tab`: Logical traversal through actionable cards and controls.
  - `Spacebar`: Toggle checkboxes, fast-pass reaction windows.
  - `Enter`: Confirm submissions, activate primary buttons.
  - `Escape`: Minimize focal dialogs to inspection banners (unless mandatory decision requires resolution).
  - `Arrow Keys`: Navigate within card grids and option lists.
- **Screen Reader Announcements**:
  - Dynamic `aria-live="polite"` region in `GameShell` announcing:
    - Turn transitions (`"Turn started for seat: Sol"`).
    - Decisions required (`"Decision required: Choose a system to activate"`).
    - Combat results (`"Combat roll: 3 hits scored against your fleet"`).
- **Contrast & Visibility**:
  - All text meets minimum 4.5:1 contrast ratio against dark backgrounds.
  - Active focus rings: `outline: 2px solid #38bdf8; outline-offset: 2px`.
  - Color is never the sole indicator of state (always paired with badges, icons, or labels).

---

## 8. Definition of Done & Acceptance Gates

A work package or milestone completion requires satisfying all of the following criteria:

1. **Zero Monolithic Radio Fallbacks**: Payments, fleet moves, casualties, trade deals, and votes use their dedicated workflow components; generic radio lists appear only for genuine 1-of-N menu choices.
2. **Strict Constraint Adherence**: `min_selection` and `max_selection` are strictly enforced by the client before submission is enabled.
3. **Hidden Information Integrity**: No opponent private holdings, secret objectives, or actor-only constraints are rendered in the DOM for other seats or spectators.
4. **Authoritative Engine Preservation**: 100% of state changes originate from engine-validated `option_id` submissions. Zero client-side rules mutation or state fabrication.
5. **Clean Verification**:
   - `npm test` passes with zero failures.
   - `npm run lint` / TypeScript check passes with zero errors.
   - Playwright multi-seat end-to-end tests complete cleanly with zero unhandled exceptions or console errors.
