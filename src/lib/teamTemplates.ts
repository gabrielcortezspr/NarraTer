// Team shapes the generator knows (`narrater team create --template …`), as
// data. The generator fills a vetted shape instead of inventing a topology:
// that is what worked in the literature (AFlow, MaAS, MasRouter route between
// known shapes) and what our own benchmark supports — see
// docs/pesquisa/orquestracao.md §4–§7 and docs/relatorios/equipe-ou-sozinho.pdf.
// "solo" is a shape too, and the default answer (D22).

export interface TeamMemberSpec {
  /** Terminal name (made unique on the canvas). */
  name: string;
  /** Agent preset key or label; default: the caller's. */
  preset?: string;
  /** Model for the agent's --model flag; default: the CLI's own. */
  model?: string;
  /** Role name shown on the tile. */
  roleName?: string;
  /** Role prompt, installed as CLAUDE.md / AGENTS.md. */
  role: string;
  /** Changes files (false: reads, reviews, reports). */
  writes: boolean;
}

export interface TeamTemplate {
  id: string;
  label: string;
  /** When this shape pays off (shown to agents and in the UI). */
  when: string;
  /** What we measured or read about its cost (shown with the team). */
  cost: string;
  /** Builds the members; `count` only for shapes with N alike members. */
  members: (count: number) => TeamMemberSpec[];
  /** Wiring by member name; "me" is the caller. Default: each member ↔ me. */
  edges?: (members: TeamMemberSpec[]) => [string, string][];
  /** Default and allowed range of `count`, for N-member shapes. */
  count?: { default: number; min: number; max: number };
}

const KNOW_TEAM =
  "Run `narrater list` to see your teammates and shared notes. The task is in the note named '<team> · task' " +
  "connected to you: read it first.";

const WRITER = (extra: string) =>
  `You implement your part of the team's task. ${KNOW_TEAM} Change only the files in your scope; never edit ` +
  `files another teammate is working on. ${extra} When you finish, answer whoever delegated to you in a few ` +
  "lines: files changed and the result of the acceptance check. Doubts about the task go to whoever delegated, never to the user.";

const REVIEWER =
  `You are a code reviewer. You never edit files. ${KNOW_TEAM} When asked for a review, read the task note, the ` +
  "project's rules, the diff (git diff) and the tests, run the tests, and answer with a short, concrete list of " +
  "defects: file, the requirement it breaks and an input that fails. If you find nothing, say it is approved. " +
  "The agent who asked is your only point of contact.";

const QA =
  `You are the team's tester. ${KNOW_TEAM} From the task note alone (not from the implementation), write tests ` +
  "that pin down the required behaviour, including edge cases the task states. You only create or change test " +
  "files. When the developer says it is ready, run your tests and answer with what fails and why; when all pass, say so.";

const READER =
  `You investigate. You never edit files. ${KNOW_TEAM} Look only at the slice of the problem you are given, and ` +
  "answer with findings and the evidence for each (file:line, command output, link). Say what you did not check.";

export const TEAM_TEMPLATES: TeamTemplate[] = [
  {
    id: "solo",
    label: "Solo",
    when:
      "The default. A task one agent finishes in minutes, sequential work, or parts that touch the same files. " +
      "Records the decision; creates no terminal.",
    cost: "1×. Measured: one Opus terminal beat every team on both benchmark tasks, with the same score.",
    members: () => [],
  },
  {
    id: "dev-reviewer",
    label: "Developer + reviewer",
    when:
      "A change that matters, where a second look pays: the developer is a cheaper model, or the change is risky. " +
      "The reviewer has a clean context (it never sees the developer's history).",
    cost:
      "Measured with an Opus developer that made no mistakes: +36% cost, +0.5 min, same score. Literature: the " +
      "reviewer pays when the writer errs (Cognition: ~2 bugs per PR).",
    members: () => [
      {
        name: "Dev",
        roleName: "Developer",
        writes: true,
        role: WRITER(
          'When you believe it is done and the acceptance check passes, ask the Reviewer: `narrater ask "Reviewer" ' +
            '"Review my changes in <project dir>"`. Fix what is valid; then answer.'
        ),
      },
      { name: "Reviewer", roleName: "Reviewer", writes: false, role: REVIEWER },
    ],
    edges: () => [["me", "Dev"], ["Dev", "Reviewer"]],
  },
  {
    id: "dev-qa",
    label: "Developer + tester",
    when:
      "New behaviour with a clear spec, where tests written independently of the code catch what the developer " +
      "would miss (MetaGPT, AgentCoder).",
    cost: "Not measured in NarraTer yet. Expect about 2× a solo run.",
    members: () => [
      {
        name: "Dev",
        roleName: "Developer",
        writes: true,
        role: WRITER('When it is ready, ask the tester to run their tests: `narrater ask "QA" "Ready: run your tests"`, and fix what fails.'),
      },
      { name: "QA", roleName: "Tester", writes: true, role: QA },
    ],
    edges: () => [["me", "Dev"], ["me", "QA"], ["Dev", "QA"]],
  },
  {
    id: "readers",
    label: "Readers fan-out",
    when:
      "Investigation, audit or comparison across independent areas that would flood one context. Readers never " +
      "write; you split the question and synthesize the answers.",
    cost: "Grows with the number of readers. Literature: the case where teams win most (Anthropic: +90% on research, ~15× tokens).",
    count: { default: 3, min: 2, max: 5 },
    members: (n) =>
      Array.from({ length: n }, (_, i) => ({ name: `Reader ${i + 1}`, roleName: "Reader", writes: false, role: READER })),
  },
  {
    id: "parallel-devs",
    label: "Parallel developers",
    when:
      "Two or more independent parts in separate files, each worth several minutes of work. Give every developer " +
      "the same model: the slowest one sets the pace.",
    cost:
      "Measured on 4 small independent modules: same cost as solo, 1.6× slower (the Haiku dev took twice as long " +
      "and the lead added ~45 s). Pays only when each part is much bigger than the ~1 min of coordination.",
    count: { default: 2, min: 2, max: 5 },
    members: (n) =>
      Array.from({ length: n }, (_, i) => ({
        name: `Dev ${i + 1}`,
        roleName: "Developer",
        writes: true,
        role: WRITER("Your scope (files or module) is the one given when the work is delegated to you."),
      })),
  },
];

export function findTemplate(ref: string): TeamTemplate | undefined {
  const wanted = ref.trim().toLowerCase();
  return TEAM_TEMPLATES.find((t) => t.id === wanted || t.label.toLowerCase() === wanted);
}

export function describeTemplates(): string {
  return TEAM_TEMPLATES.map((t) => {
    const members = t.members(t.count?.default ?? 1);
    const who = members.length
      ? members.map((m) => `${m.name} (${m.writes ? "writes" : "reads"})`).join(", ") +
        (t.count ? ` — --count ${t.count.min}–${t.count.max}` : "")
      : "no terminals";
    const wiring = members.length
      ? (t.edges?.(members) ?? members.map((m) => ["me", m.name] as [string, string]))
          .map(([a, b]) => `${a === "me" ? "you" : a} ↔ ${b}`)
          .join(", ")
      : "";
    return [
      `${t.id} — ${t.label}`,
      `  members: ${who}${wiring ? `\n  wiring:  ${wiring}` : ""}`,
      `  when:    ${t.when}`,
      `  cost:    ${t.cost}`,
    ].join("\n");
  }).join("\n\n");
}
