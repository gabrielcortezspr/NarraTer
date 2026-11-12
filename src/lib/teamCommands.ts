import { GROUND, floorOf, sceneOfNode, sliceOf, useCanvasStore, type AppNode, type TerminalNodeData } from "@/stores/canvas";
import { AGENT_ORDER, AGENT_PRESETS, presetOf, type AgentType } from "@/lib/agents";
import { createFloor, findFloor, floorDir } from "@/lib/floors";
import { freeSpot, labelOf, str, uniqueName, type CanvasRequest } from "@/lib/canvasReach";
import { describeTemplates, findTemplate, type TeamMemberSpec } from "@/lib/teamTemplates";
import { currentPreferences } from "@/stores/preferences";

// The team generator (`narrater team create`): one request builds a whole team
// next to the caller — terminals with agent, model and role, the wiring, a
// task note for everyone and a "why this team" note for the caller. It fills
// a template (teamTemplates.ts) or a JSON spec; it never removes anything
// (taking a team down is a future feature). Who may: a Maestro terminal, or
// any agent when the user turned on Automatic teams — but never a member of a
// generated team (no teams of teams). The decision rule is D22.

export const TEAM_ACTIONS = new Set(["team_templates", "team_create"]);

const TERMINAL_SIZE = { width: 520, height: 360 };
const NOTE_SIZE = { width: 340, height: 240 };
/** Most members one request may create (Agent Teams: "start with 3–5"). */
const MAX_MEMBERS = 5;

interface Member extends TeamMemberSpec {
  agentType: AgentType;
  command?: string;
  isolate: boolean;
}

function presetKey(ref: string): AgentType | undefined {
  const wanted = ref.trim().toLowerCase();
  return AGENT_ORDER.find((t) => t === wanted || AGENT_PRESETS[t].label.toLowerCase() === wanted);
}

/** Members from the spec or the template, with defaults applied and checked. */
function resolveMembers(params: Record<string, unknown>, me: TerminalNodeData): Member[] | string {
  const spec = (params.spec ?? {}) as Record<string, unknown>;
  const templateRef = str(params.template) || str(spec.template);
  const template = templateRef ? findTemplate(templateRef) : undefined;
  if (templateRef && !template) return `Error: no template named '${templateRef}' — \`narrater team templates\` lists them`;

  let raw: Partial<TeamMemberSpec>[];
  if (Array.isArray(spec.members)) {
    raw = spec.members as Partial<TeamMemberSpec>[];
  } else if (template) {
    const range = template.count;
    const count = Number(str(params.count) || spec.count || range?.default || 1);
    if (str(params.count) && !range) return `Error: --count only applies to ${["readers", "parallel-devs"].join(" and ")}`;
    if (range && (!Number.isInteger(count) || count < range.min || count > range.max)) {
      return `Error: --count for ${template.id} takes ${range.min}–${range.max}`;
    }
    raw = template.members(count);
  } else {
    return "Error: give --template or a --spec with \"members\"";
  }
  if (raw.length > MAX_MEMBERS) return `Error: at most ${MAX_MEMBERS} members per team (start small: 2–3)`;

  const defaultPreset = str(params.preset);
  const defaultModel = str(params.model).trim();
  const isolateAll = params.isolate === true;
  const out: Member[] = [];
  for (const [i, m] of raw.entries()) {
    const name = str(m.name).trim() || `Member ${i + 1}`;
    const role = str(m.role).trim();
    if (!role) return `Error: member '${name}' has no "role" (its prompt)`;
    const presetRef = str(m.preset) || defaultPreset;
    let agentType: AgentType = me.agentType ?? "claude";
    let command = presetRef ? undefined : me.command;
    if (presetRef) {
      const key = presetKey(presetRef);
      if (!key) return `Error: member '${name}': no preset named '${presetRef}' — \`narrater preset list\``;
      if (key === "shell" || key === "custom") return `Error: member '${name}': a team member must be an agent (Claude Code, Codex, Gemini CLI, OpenCode)`;
      agentType = key;
    }
    if (agentType === "shell") return "Error: a shell can't generate a team of agents — run this from an agent";
    const model = str(m.model).trim() || defaultModel || (!presetRef ? me.model : undefined);
    if (model && !AGENT_PRESETS[agentType].modelFlag) return `Error: member '${name}': ${presetOf(agentType).label} takes no --model`;
    const writes = m.writes !== false;
    out.push({ name, role, roleName: str(m.roleName) || undefined, writes, agentType, command, model: model || undefined, isolate: isolateAll && writes });
  }
  const names = out.map((m) => m.name.toLowerCase());
  if (new Set(names).size !== names.length) return "Error: two members share a name";
  return out;
}

/** Wiring by member name ("me" = the caller); default: a star around the caller. */
function resolveEdges(params: Record<string, unknown>, members: Member[]): [string, string][] | string {
  const spec = (params.spec ?? {}) as Record<string, unknown>;
  const template = findTemplate(str(params.template) || str(spec.template));
  let edges: [string, string][];
  if (Array.isArray(spec.edges)) edges = spec.edges as [string, string][];
  else if (template?.edges && !Array.isArray(spec.members)) edges = template.edges(members);
  else edges = members.map((m) => ["me", m.name]);
  const known = new Set(["me", ...members.map((m) => m.name.toLowerCase())]);
  for (const e of edges) {
    if (!Array.isArray(e) || e.length !== 2 || !e.every((x) => known.has(str(x).toLowerCase()))) {
      return `Error: bad edge ${JSON.stringify(e)} — use member names or "me"`;
    }
  }
  // Every member must be reachable from the caller, or nobody can brief it
  const linked = new Set(["me"]);
  for (let grew = true; grew; ) {
    grew = false;
    for (const [a, b] of edges.map(([x, y]) => [x.toLowerCase(), y.toLowerCase()])) {
      if (linked.has(a) !== linked.has(b)) {
        linked.add(a);
        linked.add(b);
        grew = true;
      }
    }
  }
  const lost = members.filter((m) => !linked.has(m.name.toLowerCase()));
  if (lost.length) return `Error: ${lost.map((m) => m.name).join(", ")} wouldn't be reachable from you — add an edge`;
  return edges;
}

function taskNote(team: string, task: string, check: string, members: Member[], wiring: string): string {
  const lines = [`# ${team} · task`, "", task || "(the task comes with each delegation)"];
  if (check) {
    lines.push("", `**Acceptance:** \`${check}\` must pass in the project. NarraTer runs it after each delegated turn ` +
      "(`narrater ask … --check`), so a \"done\" that fails it comes back as failed.");
  }
  lines.push("", "**Team**");
  members.forEach((m) => lines.push(`- ${m.name} — ${m.roleName ?? "member"}, ${m.writes ? "writes" : "reads only"}`));
  lines.push("", `**Wiring:** ${wiring}`, "", "Two members never write the same files.");
  return lines.join("\n");
}

/** `base`, or `base (2)`… — free when no note is named after it yet. */
function freeTeamName(nodes: AppNode[], base: string): string {
  const notes = new Set(nodes.filter((n) => n.type === "note").map(labelOf));
  const taken = (t: string) => notes.has(`${t} · task`) || notes.has(`${t} · why`);
  let name = base;
  for (let i = 2; taken(name); i++) name = `${base} (${i})`;
  return name;
}

async function create(req: CanvasRequest): Promise<string> {
  const scene = sceneOfNode(req.from);
  const sl = sliceOf(scene);
  const me = sl?.nodes.find((n) => n.id === req.from);
  if (!sl || !scene || me?.type !== "terminal") return "Error: unknown terminal";
  const meData = me.data as TerminalNodeData;
  const params = req.params ?? {};
  const spec = (params.spec ?? {}) as Record<string, unknown>;

  // Who may (see the header)
  const prefs = await currentPreferences();
  if (meData.team && !meData.maestro) {
    return `Error: you are a member of team '${meData.team}' — members of a generated team can't generate teams`;
  }
  if (!meData.maestro && !prefs.auto_teams) {
    return "Error: team generation is off for you — the user can turn on Automatic teams in NarraTer (the team " +
      "button in the toolbar), or Maestro mode on your terminal (the crown in its header)";
  }

  const members = resolveMembers(params, meData);
  if (typeof members === "string") return members;
  const templateId = str(params.template) || str(spec.template) || "custom";
  const template = findTemplate(templateId);
  const reason = (str(params.reason) || str(spec.reason)).trim();
  const task = (str(params.task) || str(spec.task)).trim();
  const check = (str(params.check) || str(spec.check)).trim();
  const store = useCanvasStore.getState();
  const teamName = freeTeamName(sl.nodes, (str(params.name) || str(spec.name) || template?.label || "Team").trim());
  const myFloor = floorOf(me);
  const floorNodes = () => (sliceOf(scene)?.nodes ?? []).filter((n) => floorOf(n) === myFloor);
  const addNote = (label: string, content: string) =>
    store.addNoteNode(freeSpot(floorNodes(), me, NOTE_SIZE), { content, label: uniqueName(sliceOf(scene)?.nodes ?? [], "note", label) }, scene);
  const link = (a: string, b: string, type: string) =>
    store.addEdge({ id: `edge-${crypto.randomUUID().slice(0, 8)}`, source: a, target: b, type }, scene);
  const stamp = new Date().toISOString().slice(0, 16).replace("T", " ");

  // Solo is a decision, not a team: record it and stop
  if (members.length === 0) {
    const id = addNote(`${teamName} · why`, [
      `# ${teamName} · why`, "", "**Decision:** solo — no team.", reason ? `**Reason:** ${reason}` : "",
      template ? `**Expected cost:** ${template.cost}` : "", `**By:** ${labelOf(me)}, ${stamp}`,
    ].filter(Boolean).join("\n"));
    link(req.from, id, "agent-note");
    return `ok: recorded the decision to work solo in the note '${teamName} · why'. No terminal was created — do the task yourself.`;
  }
  if (!reason) {
    return "Error: say why a team beats doing it alone with --reason (2+ independent parts in separate files, an " +
      "independent reviewer or tester, or work that doesn't fit one context). Otherwise work solo (D22).";
  }
  const edges = resolveEdges(params, members);
  if (typeof edges === "string") return edges;

  // Isolated writers first: a floor (git worktree) each, so a failure stops before any terminal exists
  const floorOfMember = new Map<string, { id: string; dir: string | null }>();
  for (const m of members.filter((x) => x.isolate)) {
    try {
      const { name } = await createFloor({ name: `${teamName} · ${m.name}` }, scene);
      const f = findFloor(name, scene);
      if (f && f !== "ground") floorOfMember.set(m.name, { id: f.id, dir: floorDir(f.id, scene) });
    } catch (e) {
      return `Error: could not give ${m.name} its own worktree (${e}); run without --isolate, or from a git project`;
    }
  }

  const teamId = `team-${crypto.randomUUID().slice(0, 6)}`;
  const idOf = new Map<string, string>([["me", req.from]]);
  const created: AppNode[] = [];
  for (const m of members) {
    const iso = floorOfMember.get(m.name);
    const nodes = iso ? (sliceOf(scene)?.nodes ?? []).filter((n) => floorOf(n) === iso.id) : floorNodes();
    const pos = iso && nodes.length === 0 ? { x: me.position.x, y: me.position.y } : freeSpot(nodes, me, TERMINAL_SIZE);
    const label = uniqueName(sliceOf(scene)?.nodes ?? [], "terminal", m.name);
    const id = store.addTerminalNode(
      {
        agentType: m.agentType,
        command: m.command,
        model: m.model,
        label,
        roleName: m.roleName,
        instructions: m.role.split("<team>").join(teamName),
        cwd: iso?.dir ?? meData.cwd,
        team: teamName,
        floor: iso ? iso.id : myFloor === GROUND ? undefined : myFloor,
      },
      pos,
      scene,
    );
    idOf.set(m.name.toLowerCase(), id);
    const node = sliceOf(scene)?.nodes.find((n) => n.id === id);
    if (node) created.push(node);
  }
  for (const [a, b] of edges) {
    link(idOf.get(a.toLowerCase())!, idOf.get(b.toLowerCase())!, "agent-pipe");
  }

  const name = (id: string) => (id === req.from ? "you" : labelOf(created.find((n) => n.id === id)!));
  const wiring = edges.map(([a, b]) => `${name(idOf.get(a.toLowerCase())!)} ↔ ${name(idOf.get(b.toLowerCase())!)}`).join(", ");
  const finalMembers = members.map((m) => ({ ...m, name: labelOf(created.find((n) => n.id === idOf.get(m.name.toLowerCase()))!) }));

  const taskId = addNote(`${teamName} · task`, taskNote(teamName, task, check, finalMembers, wiring));
  [req.from, ...created.map((n) => n.id)].forEach((id) => link(id, taskId, "agent-note"));
  const whyId = addNote(`${teamName} · why`, [
    `# ${teamName} · why`, "",
    `**Template:** ${template ? `${template.id} (${template.label})` : "custom spec"}`,
    `**Reason:** ${reason}`,
    template ? `**Expected cost:** ${template.cost}` : "",
    `**By:** ${labelOf(me)}${meData.maestro ? " (Maestro)" : " (Automatic teams)"}, ${stamp}`,
  ].filter(Boolean).join("\n"));
  link(req.from, whyId, "agent-note");

  const who = finalMembers
    .map((m) => `${m.name} (${presetOf(m.agentType).label}${m.model ? `, ${m.model}` : ""}${m.isolate ? ", own worktree" : ""}) — ${m.writes ? "writes" : "reads"}`)
    .join("; ");
  const first = finalMembers.find((m) => m.writes) ?? finalMembers[0];
  return [
    `ok: team '${teamName}' created — ${who}.`,
    `Wiring: ${wiring}. Notes: '${teamName} · task' (everyone), '${teamName} · why' (you).`,
    `Next: delegate with \`narrater ask "${first.name}" "Read the note '${teamName} · task' and do your part"` +
      `${check ? ` --check "${check}"` : ""}\`. They may still be booting; the first ask waits.`,
  ].join("\n");
}

export async function handleTeam(req: CanvasRequest): Promise<string> {
  if (req.action === "team_templates") {
    const prefs = await currentPreferences();
    const me = sliceOf(sceneOfNode(req.from))?.nodes.find((n) => n.id === req.from);
    const d = me?.data as TerminalNodeData | undefined;
    const may = d?.team && !d.maestro ? "no (you are a member of a generated team)"
      : d?.maestro ? "yes (Maestro)"
      : prefs.auto_teams ? "yes (Automatic teams is on)" : "no (Automatic teams is off and you are not the Maestro)";
    return `${describeTemplates()}\n\nYou may generate a team: ${may}.\nDefault answer: solo (D22). See \`narrater help team\`.`;
  }
  return create(req);
}
