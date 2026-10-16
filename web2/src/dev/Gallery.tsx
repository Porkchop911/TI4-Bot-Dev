import { useState, type ReactNode } from "react";
import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeading,
  CardText,
  Hint,
  ChipTabs,
  Counter,
  Die,
  Eyebrow,
  Gauge,
  Gauges,
  ICON_NAMES,
  Icon,
  InlineNote,
  Kbd,
  ListRow,
  Menu,
  Offer,
  Pill,
  Quantity,
  Segmented,
  Select,
  StepTabs,
  SummaryGrid,
  SummaryStat,
  Trail,
} from "../ui";

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-3">
      <h2 className="border-b border-line pb-1 text-muted">
        <Eyebrow>{title}</Eyebrow>
      </h2>
      {children}
    </section>
  );
}
const Line = ({ children }: { children: ReactNode }) => (
  <div className="flex flex-wrap items-center gap-2">{children}</div>
);

/** Every design primitive in every variant, for visual review. Open with `?gallery`. */
export function Gallery() {
  const [count, setCount] = useState(2);
  const [view, setView] = useState("b");
  const [step, setStep] = useState(1);
  const accents = [
    ["Draft", "var(--color-gold)"],
    ["Live", "var(--color-cyan)"],
    ["Complete", "var(--color-green)"],
  ];
  return (
    <div className="h-dvh overflow-auto p-6">
      <h1 className="mb-4 text-lg font-semibold">Design primitives</h1>
      <div className="grid gap-8 lg:grid-cols-3">
        {accents.map(([name, accent]) => (
          <div
            key={name}
            className="space-y-6"
            style={{ "--accent": accent } as React.CSSProperties}
          >
            <h2 className="text-md font-semibold text-accent">Accent · {name}</h2>
            <Group title="Buttons">
              <Line>
                <Button>Default</Button>
                <Button tone="primary" iconAfter="arrow">
                  Primary
                </Button>
                <Button tone="quiet">Quiet</Button>
                <Button tone="demo">Simulate (demo)</Button>
                <Button tone="primary" disabled>
                  Disabled
                </Button>
              </Line>
              <Line>
                <Button size="sm">Small</Button>
                <Button size="sm" tone="quiet" icon="edit">
                  Edit movement
                </Button>
                <Button size="sm" tone="quiet" active>
                  Active
                </Button>
                <Button size="icon" tone="quiet" icon="undo" aria-label="Undo" />
                <Menu
                  label="More"
                  items={[
                    { label: "Start over", onSelect() {} },
                    { label: "Discard draft", onSelect() {} },
                  ]}
                />
                <Select defaultValue="a">
                  <option value="a">Direct</option>
                  <option value="b">via #40</option>
                </Select>
              </Line>
            </Group>
            <Group title="Badges, pills, dice">
              <Line>
                {(["draft", "live", "done", "alert", "quiet"] as const).map((tone) => (
                  <Badge key={tone} tone={tone}>
                    {tone}
                  </Badge>
                ))}
              </Line>
              <Line>
                <Button>
                  Choose <Kbd>t</Kbd>
                </Button>
                <Button tone="quiet">
                  Back to actions <Kbd>Escape</Kbd>
                </Button>
                <Button tone="primary" iconAfter="arrow">
                  End turn <Kbd>Enter</Kbd>
                </Button>
              </Line>
              <Line>
                <Pill>plain</Pill>
                <Pill tone="damage">1 damaged</Pill>
                <Pill tone="loss">−1</Pill>
                <Pill tone="staged">sustain</Pill>
                <Die roll={9} hit />
                <Die roll={3} hit={false} />
                <Die />
              </Line>
              <Trail
                items={[
                  { label: "Move ships", status: "complete" },
                  { label: "Load cargo", status: "active" },
                  { label: "Control", status: "todo" },
                  { label: "Space cannon offense", status: "skipped", reason: "no PDS" },
                ]}
              />
            </Group>
            <Group title="Steps">
              <StepTabs
                label="Steps"
                selected={step}
                current={2}
                onSelect={setStep}
                tabs={[
                  { name: "Activation", status: "done", caption: "#27 Starpoint" },
                  { name: "Movement", status: "needs-review", caption: "Needs review" },
                  { name: "Space combat", status: "decision", caption: "Your decision" },
                  { name: "Invasion", status: "skipped", caption: "Skipped" },
                  { name: "Production", status: "future", caption: "" },
                ]}
              />
            </Group>
            <Group title="Cards and rows">
              <Card>
                <CardHeading title="Jord · #1">1 system away · Capacity 2 / 4</CardHeading>
                <ListRow
                  icon="carrier"
                  title="Carrier"
                  subtitle="Move 1 · Capacity 4 · 1 available"
                  controls={<Counter value={count} max={4} label="Carrier" onChange={setCount} />}
                />
                <ListRow
                  icon="dreadnought"
                  title="Dreadnought"
                  subtitle="Placed in the space area"
                  controls={<Quantity count={1} />}
                />
                <ListRow
                  icon="cruiser"
                  title="Cruiser"
                  subtitle="No longer at Jord"
                  invalid
                  controls={<Button size="sm">Remove from fleet</Button>}
                />
                <ListRow
                  icon="infantry"
                  title="Infantry"
                  subtitle="Planet Jord · 1 capacity each"
                  linked
                  controls={
                    <Segmented
                      label="Placement"
                      options={[
                        { id: "a", label: "Starpoint" },
                        { id: "b", label: "In space" },
                      ]}
                      value={view}
                      onChange={setView}
                    />
                  }
                />
              </Card>
              <Card>
                <CardHeading title="Empty card" />
                <CardText>Nothing produced.</CardText>
                <CardBody>
                  <ChipTabs
                    label="Rolls"
                    options={[
                      { id: "a", label: "Barrage" },
                      { id: "b", label: "Round 1" },
                      { id: "c", label: "Round 2 · next" },
                    ]}
                    value={view}
                    onChange={setView}
                  />
                </CardBody>
              </Card>
              <SummaryGrid>
                <SummaryStat eyebrow="Active system" title="#27 Starpoint">
                  None of your command tokens were here.
                </SummaryStat>
              </SummaryGrid>
            </Group>
            <Group title="Notes and gauges">
              <InlineNote strong="Costs 1 tactic token.">
                Dice and later decisions continue in Live.
              </InlineNote>
              <InlineNote tone="success" strong="Resolved.">
                Blair gained 3 trade goods.
              </InlineNote>
              <InlineNote tone="error">Cargo exceeds transport capacity by 1.</InlineNote>
              <InlineNote tone="quiet">No ground combat expected here.</InlineNote>
              <Offer
                eyebrow="Action card · Start of a combat round"
                title="Morale Boost"
                hint={
                  <Hint label="About Morale Boost">
                    Apply +1 to the result of each of your unit’s combat rolls during this round.
                  </Hint>
                }
                actions={<Button>Play Morale Boost</Button>}
              />
              <Gauges>
                <Gauge label="Fleet supply" used={2} total={3} />
                <Gauge label="Transport capacity" used={5} total={4} />
              </Gauges>
            </Group>
          </div>
        ))}
      </div>
      <div className="mt-8 flex flex-wrap gap-4 text-xs text-muted">
        {ICON_NAMES.map((name) => (
          <span key={name} className="flex items-center gap-1.5">
            <Icon name={name} className="size-[22px] text-icon" />
            {name}
          </span>
        ))}
      </div>
    </div>
  );
}
