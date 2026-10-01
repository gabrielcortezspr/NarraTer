import { useCanvasStore } from "@/stores/canvas";
import { useRolesStore } from "@/stores/roles";
import { toast } from "@/stores/toasts";
import { fsListDir, systemNotify, type Role } from "@/lib/tauri";
import { AGENT_ORDER, AGENT_PRESETS, presetOf, type AgentType } from "@/lib/agents";
import { findByName, freeSpot, labelOf, neighbors, reachOf, str, uniqueName, type CanvasRequest } from "@/lib/canvasReach";
import type { AppNode, TerminalNodeData } from "@/stores/canvas";

// Maestro commands of the `narrater` CLI (Maestri's maestri-manager skill):
// recruit/dismiss teammates, presets, roles and desktop notifications. They
// only work from a terminal in Maestro mode — the user opts in per terminal.

const RECRUIT_SIZE = { width: 520, height: 360 };
const ROLE_COLORS = ["#8b5cf6", "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#ec4899", "#14b8a6", "#a3e635"];

const NOT_MAESTRO =
  "Error: this terminal is not the Maestro — the user must turn on Maestro mode on your terminal " +
  "(the crown in its header) to unlock this command";

export const MANAGER_ACTIONS = new Set([
  "recruit", "dismiss", "preset_list", "notify",
  "role_list", "role_show", "role_create", "role_write", "role_edit", "role_delete", "role_assign",
]);

interface Ctx {
  req: CanvasRequest;
  params: Record<string, unknown>;
  me: AppNode;
  meData: TerminalNodeData;
}

/** Preset by type key or label ("codex", "Gemini CLI"…), case-insensitive. */
function findPreset(ref: string): AgentType | undefined {
  const wanted = ref.trim().toLowerCase();
  return AGENT_ORDER.find((t) => t === wanted || AGENT_PRESETS[t].label.toLowerCase() === wanted);
}

/**
 * Which agent a recruit runs: an explicit preset, a bare command (typed as
 * the matching preset when it is exactly that program, Custom otherwise), or
 * — with neither — a copy of the caller.
 */
function resolveAgent(ctx: Ctx): { agentType: AgentType; command?: string } | string {
  const presetRef = str(ctx.params.preset);
  const command = str(ctx.params.command).trim();
  if (presetRef) {
    const type = findPreset(presetRef);
    if (!type) return `Error: no preset named '${presetRef}' — run \`narrater preset list\``;
    if (type === "custom" && !command) return "Error: the Custom preset needs --command";
    return { agentType: type, command: type === "custom" ? command : undefined };
  }
  if (command) {
    const match = AGENT_ORDER.find((t) => AGENT_PRESETS[t].command === command);
    return match ? { agentType: match } : { agentType: "custom", command };
  }
  return { agentType: ctx.meData.agentType ?? "shell", command: ctx.meData.command };
}

async function loadedRoles(): Promise<Role[]> {
  const store = useRolesStore.getState();
  if (!store.loaded) await store.load();
  return useRolesStore.getState().roles;
}

async function findRole(name: string): Promise<Role | string> {
  const roles = await loadedRoles();
  const wanted = name.trim().toLowerCase();
  const role = roles.find((r) => r.name.toLowerCase() === wanted);
  return role ?? `Error: no role named '${name}' — run \`narrater role list\`, or \`narrater role create\` first`;
}

function rolePatch(role: Role | null): Partial<TerminalNodeData> {
  return role
    ? { roleId: role.id, roleName: role.name, roleColor: role.color, instructions: role.instructions }
    : { roleId: undefined, roleName: undefined, roleColor: undefined, instructions: undefined };
}

/** Directory exists? (`~` is expanded by the backend.) */
async function checkDir(dir: string): Promise<string | null> {
  try {
    await fsListDir(dir);
    return null;
  } catch {
    return `Error: '${dir}' is not a directory that exists — create it first`;
  }
}

function describe(d: TerminalNodeData): string {
  const kind = d.agentType === "custom" && d.command ? d.command : presetOf(d.agentType).label;
  if (!d.roleName) return kind;
  // Roles are instruction files for agents — a shell has nothing to read them
  return d.agentType === "shell" ? `${kind} · role '${d.roleName}' not applied to shells` : `${kind} · role: ${d.roleName}`;
}

async function recruit(ctx: Ctx): Promise<string> {
  const store = useCanvasStore.getState();
  const agent = resolveAgent(ctx);
  if (typeof agent === "string") return agent;
  const roleRef = str(ctx.params.role);
  const role = roleRef ? await findRole(roleRef) : null;
  if (typeof role === "string") return role;
  const dir = str(ctx.params.dir).trim();
  if (dir) {
    const bad = await checkDir(dir);
    if (bad) return bad;
  }
  const name = str(ctx.params.name).trim();
  const replaceRef = str(ctx.params.replace).trim();

  if (replaceRef) {
    if (!str(ctx.params.preset) && !str(ctx.params.command)) {
      return "Error: --replace swaps the agent, so it needs --preset (or --command)";
    }
    const { agents } = reachOf(store.nodes, store.edges, ctx.req.from);
    const target = findByName(agents, replaceRef, "agent");
    if (typeof target === "string") return target;
    const oldName = labelOf(target);
    const patch: Partial<TerminalNodeData> = {
      agentType: agent.agentType,
      command: agent.command,
      skipPermissions: undefined,
      ...(role ? rolePatch(role) : {}),
      ...(dir ? { cwd: dir } : {}),
      ...(name ? { label: uniqueName(store.nodes, "terminal", name, target.id) } : {}),
    };
    store.replaceTerminalAgent(target.id, patch);
    const now = useCanvasStore.getState().nodes.find((n) => n.id === target.id);
    const d = now?.data as TerminalNodeData;
    return (
      `ok: '${oldName}' now runs ${describe(d)}${name ? ` as '${labelOf(now!)}'` : ""} — same node, ` +
      "connections and position kept. Its chat history did not survive the restart: brief it with `narrater ask`."
    );
  }

  const label = uniqueName(store.nodes, "terminal", name || presetOf(agent.agentType).label);
  const id = store.addTerminalNode(
    {
      ...agent,
      label,
      ...(role ? rolePatch(role) : {}),
      cwd: dir || ctx.meData.cwd,
    },
    freeSpot(store.nodes, ctx.me, RECRUIT_SIZE),
  );
  store.addEdge({ id: `edge-${crypto.randomUUID().slice(0, 8)}`, source: ctx.req.from, target: id, type: "agent-pipe" });
  const d = useCanvasStore.getState().nodes.find((n) => n.id === id)?.data as TerminalNodeData;
  return (
    `ok: recruited '${label}' (${describe(d)}), connected to you. It may still be booting — ` +
    `your first \`narrater ask "${label}" "..."\` waits until it's ready.`
  );
}

function dismiss(ctx: Ctx): string {
  const store = useCanvasStore.getState();
  const { agents } = reachOf(store.nodes, store.edges, ctx.req.from);
  const target = findByName(agents, str(ctx.params.name), "agent");
  if (typeof target === "string") return target;
  // Notes wired only to this agent lose their last link (only the user can rewire them)
  const orphans = neighbors(store.edges, target.id)
    .map((id) => store.nodes.find((n) => n.id === id))
    .filter((n): n is AppNode => n?.type === "note" && neighbors(store.edges, n.id).length === 1)
    .map(labelOf);
  store.removeNode(target.id);
  return (
    `ok: dismissed '${labelOf(target)}'` +
    (orphans.length ? ` — note(s) ${orphans.map((n) => `'${n}'`).join(", ")} are no longer connected to anyone` : "")
  );
}

function presetList(): string {
  return AGENT_ORDER.map((t) => {
    const p = AGENT_PRESETS[t];
    return `- ${p.label}${p.command ? ` (${p.command})` : ""} — ${p.description}`;
  }).join("\n");
}

async function roleList(): Promise<string> {
  const roles = await loadedRoles();
  if (roles.length === 0) return "(no roles — `narrater role create \"Name\" \"Prompt\"` adds one)";
  return roles
    .map((r) => {
      const preview = r.instructions.replace(/\s+/g, " ").trim();
      const short = preview.length > 90 ? `${preview.slice(0, 89)}…` : preview;
      return `- ${r.name}${r.orchestrator ? " [delegate-only]" : ""}: ${short}`;
    })
    .join("\n");
}

async function roleCommand(ctx: Ctx): Promise<string> {
  const rolesStore = useRolesStore.getState();
  const { action, params } = { action: ctx.req.action, params: ctx.params };
  const name = str(params.name).trim();

  if (action === "role_list") return roleList();

  if (action === "role_create") {
    const prompt = str(params.prompt).trim();
    if (!name || !prompt) return 'Usage: narrater role create "Name" "Prompt"';
    const roles = await loadedRoles();
    if (roles.some((r) => r.name.toLowerCase() === name.toLowerCase())) {
      return `Error: a role named '${name}' already exists — use \`narrater role write\` to change it`;
    }
    await rolesStore.addRole({
      name,
      instructions: prompt,
      color: ROLE_COLORS[roles.length % ROLE_COLORS.length],
      orchestrator: false,
    });
    return `ok: role '${name}' created — recruit with \`narrater recruit "<codename>" --role "${name}"\``;
  }

  if (action === "role_assign") {
    const store = useCanvasStore.getState();
    const { agents } = reachOf(store.nodes, store.edges, ctx.req.from);
    const target = findByName(agents, str(params.recruit), "agent");
    if (typeof target === "string") return target;
    const role = params.none === true ? null : await findRole(name);
    if (typeof role === "string") return role;
    store.replaceTerminalAgent(target.id, rolePatch(role));
    return (
      `ok: '${labelOf(target)}' ${role ? `now has the role '${role.name}'` : "has no role now"} — its process ` +
      "restarted (connections kept, chat history lost): brief it with `narrater ask`."
    );
  }

  const role = await findRole(name);
  if (typeof role === "string") return role;

  switch (action) {
    case "role_show":
      return `${role.name}${role.orchestrator ? " [delegate-only]" : ""}\n\n${role.instructions}`;
    case "role_write": {
      const prompt = str(params.prompt).trim();
      if (!prompt) return "Error: empty prompt";
      await rolesStore.updateRole(role.id, { instructions: prompt });
      return `ok: role '${role.name}' rewritten — running teammates pick it up on their next restart (\`narrater role assign\`)`;
    }
    case "role_edit": {
      const oldText = str(params.old);
      if (!oldText) return "Error: the text to replace is empty";
      const count = role.instructions.split(oldText).length - 1;
      if (count === 0) return `Error: text not found in role '${role.name}' — \`narrater role show\` prints it`;
      if (count > 1) return `Error: the text appears ${count} times — include more context to make it unique`;
      await rolesStore.updateRole(role.id, { instructions: role.instructions.replace(oldText, () => str(params.new)) });
      return `ok: role '${role.name}' edited`;
    }
    case "role_delete": {
      const store = useCanvasStore.getState();
      const users = store.nodes.filter((n) => n.type === "terminal" && (n.data as TerminalNodeData).roleId === role.id);
      if (users.length && params.force !== true) {
        return (
          `Error: role '${role.name}' is assigned to ${users.map((n) => `'${labelOf(n)}'`).join(", ")} — ` +
          "pass --force to delete it and unassign it from them"
        );
      }
      // Unassigned on the canvas; running agents keep their context until restarted
      users.forEach((n) => store.updateNodeData(n.id, { roleId: undefined, roleName: undefined, roleColor: undefined }));
      await rolesStore.deleteRole(role.id);
      return `ok: role '${role.name}' deleted${users.length ? ` and unassigned from ${users.length} terminal(s)` : ""}`;
    }
    default:
      return `Error: unknown role command`;
  }
}

async function notify(ctx: Ctx): Promise<string> {
  const message = str(ctx.params.message).trim();
  if (!message) return 'Usage: narrater notify "message"';
  toast.success(`${labelOf(ctx.me)}: ${message}`);
  try {
    await systemNotify(`NarraTer · ${labelOf(ctx.me)}`, message);
  } catch (e) {
    return `ok: shown in NarraTer (desktop notification failed: ${e})`;
  }
  return "ok: notification sent";
}

export async function handleManager(req: CanvasRequest): Promise<string> {
  const me = useCanvasStore.getState().nodes.find((n) => n.id === req.from);
  if (me?.type !== "terminal") return "Error: unknown terminal";
  const meData = me.data as TerminalNodeData;
  if (!meData.maestro) return NOT_MAESTRO;
  const ctx: Ctx = { req, params: req.params ?? {}, me, meData };

  switch (req.action) {
    case "recruit":
      return recruit(ctx);
    case "dismiss":
      return dismiss(ctx);
    case "preset_list":
      return presetList();
    case "notify":
      return notify(ctx);
    default:
      return roleCommand(ctx);
  }
}
