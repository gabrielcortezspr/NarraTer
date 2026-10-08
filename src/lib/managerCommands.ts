import { GROUND, floorOf, sceneOfNode, sliceOf, useCanvasStore, type SceneSlice } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import {
  FLOOR_STATES, createFloor, deleteFloorFully, describeStatus, findFloor, floorDir, floorName, landFloor,
  parseProgress, postStatus, statusOf,
} from "@/lib/floors";
import { useRolesStore } from "@/stores/roles";
import { toast } from "@/stores/toasts";
import { fsListDir, linkAdd, linksList, systemNotify, type FloorState, type Role } from "@/lib/tauri";
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
  "recruit", "dismiss", "preset_list", "notify", "floor_create", "floor_list", "floor_land", "floor_delete",
  "floor_status",
  "role_list", "role_show", "role_create", "role_write", "role_edit", "role_delete", "role_assign",
]);

interface Ctx {
  req: CanvasRequest;
  params: Record<string, unknown>;
  me: AppNode;
  meData: TerminalNodeData;
  /** The caller's scene (on screen, or running in the background) */
  scene: string;
  /** Its contents, read when the request arrived */
  sl: SceneSlice;
}

/** Fresh contents of the caller's scene (after a mutation). */
const fresh = (ctx: Ctx) => sliceOf(ctx.scene) ?? ctx.sl;

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
  const sl = ctx.sl;
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
    if (str(ctx.params.workspace).trim()) return "Error: --replace swaps the agent in place — --workspace doesn't apply";
    const { agents } = reachOf(sl.nodes, sl.edges, ctx.req.from);
    const target = findByName(agents, replaceRef, "agent");
    if (typeof target === "string") return target;
    const oldName = labelOf(target);
    const patch: Partial<TerminalNodeData> = {
      agentType: agent.agentType,
      command: agent.command,
      skipPermissions: undefined,
      ...(role ? rolePatch(role) : {}),
      ...(dir ? { cwd: dir } : {}),
      ...(name ? { label: uniqueName(sl.nodes, "terminal", name, target.id) } : {}),
    };
    store.replaceTerminalAgent(target.id, patch);
    const now = fresh(ctx).nodes.find((n) => n.id === target.id);
    const d = now?.data as TerminalNodeData;
    return (
      `ok: '${oldName}' now runs ${describe(d)}${name ? ` as '${labelOf(now!)}'` : ""} — same node, ` +
      "connections and position kept. Its chat history did not survive the restart: brief it with `narrater ask`."
    );
  }

  const floorRef = str(ctx.params.floor).trim();
  // --workspace: the recruit joins another workspace — that project's agent
  const wsRef = str(ctx.params.workspace).trim();
  if (wsRef) {
    const list = useWorkspacesStore.getState().list;
    const ws = list.find((w) => w === wsRef) ?? list.find((w) => w.toLowerCase() === wsRef.toLowerCase());
    if (!ws) return `Error: no workspace named '${wsRef}' — \`narrater workspace list\` shows them`;
    if (ws !== ctx.scene) return recruitInWorkspace(ctx, ws, agent, role, dir, name, floorRef);
  }

  // --floor: another level of this scene; on a git-isolated floor the
  // recruit works in the floor's worktree unless --dir says otherwise
  let floorId = floorOf(ctx.me);
  if (floorRef) {
    const f = findFloor(floorRef, ctx.scene);
    if (!f) return `Error: no floor named '${floorRef}' — \`narrater floor list\``;
    floorId = f === "ground" ? GROUND : f.id;
  }
  const floorNodes = sl.nodes.filter((n) => floorOf(n) === floorId);
  const cwd = dir || (floorRef ? floorDir(floorId, ctx.scene) ?? undefined : ctx.meData.cwd);

  const label = uniqueName(sl.nodes, "terminal", name || presetOf(agent.agentType).label);
  const id = store.addTerminalNode(
    {
      ...agent,
      label,
      ...(role ? rolePatch(role) : {}),
      cwd,
      floor: floorId === GROUND ? undefined : floorId,
    },
    freeSpot(floorNodes, ctx.me, RECRUIT_SIZE),
    ctx.scene,
  );
  store.addEdge({ id: `edge-${crypto.randomUUID().slice(0, 8)}`, source: ctx.req.from, target: id, type: "agent-pipe" }, ctx.scene);
  const d = fresh(ctx).nodes.find((n) => n.id === id)?.data as TerminalNodeData;
  const where = floorRef ? ` on floor '${floorRef}'${cwd ? ` (working in ${cwd})` : ""}` : "";
  return (
    `ok: recruited '${label}' (${describe(d)})${where}, connected to you. It may still be booting — ` +
    `your first \`narrater ask "${label}" "..."\` waits until it's ready.`
  );
}

/**
 * Recruits into another workspace (Maestri's `recruit --workspace`): the
 * workspace starts running if it wasn't, the terminal joins its ground (or
 * `--floor` there) working in that project's directory, and a
 * cross-workspace link wires it to the caller — addressed as `Name @ Workspace`.
 */
async function recruitInWorkspace(
  ctx: Ctx,
  ws: string,
  agent: { agentType: AgentType; command?: string },
  role: Role | null,
  dir: string,
  name: string,
  floorRef: string,
): Promise<string> {
  const store = useCanvasStore.getState();
  await store.wakeScene(ws);
  const sl = sliceOf(ws);
  if (!sl) return `Error: workspace '${ws}' couldn't be started`;
  let floorId = GROUND;
  if (floorRef) {
    const f = findFloor(floorRef, ws);
    if (!f) return `Error: workspace '${ws}' has no floor named '${floorRef}'`;
    floorId = f === "ground" ? GROUND : f.id;
  }
  const cwd = dir || floorDir(floorId, ws) || undefined;
  const floorNodes = sl.nodes.filter((n) => floorOf(n) === floorId);
  // Next to the last thing on that floor, or at the origin of an empty one
  const anchor = floorNodes[floorNodes.length - 1] ?? ({ position: { x: 0, y: 0 }, width: 0, height: 0 } as unknown as AppNode);
  const label = uniqueName(sl.nodes, "terminal", name || presetOf(agent.agentType).label);
  const id = store.addTerminalNode(
    {
      ...agent,
      label,
      ...(role ? rolePatch(role) : {}),
      cwd,
      floor: floorId === GROUND ? undefined : floorId,
    },
    floorNodes.length ? freeSpot(floorNodes, anchor, RECRUIT_SIZE) : { x: 80, y: 80 },
    ws,
  );
  await linkAdd({ id: ctx.req.from, scene: ctx.scene, label: labelOf(ctx.me) }, { id, scene: ws, label });
  const d = sliceOf(ws)?.nodes.find((n) => n.id === id)?.data as TerminalNodeData;
  const floorPart = floorRef ? ` on floor '${floorRef}'` : "";
  return (
    `ok: recruited '${label}' (${describe(d)}) in workspace '${ws}'${floorPart}` +
    `${cwd ? ` (working in ${cwd})` : ""}, linked to you — address it as \`${label} @ ${ws}\`. ` +
    `It may still be booting — your first \`narrater ask "${label} @ ${ws}" "..."\` waits until it's ready.`
  );
}

/** A teammate in another workspace linked to the caller, by `Name @ Workspace` or bare name. */
async function remoteTeammate(ctx: Ctx, ref: string): Promise<{ id: string; label: string; scene: string } | null> {
  const wanted = ref.trim().toLowerCase();
  const [namePart, scenePart] = wanted.includes("@") ? wanted.split("@").map((x) => x.trim()) : [wanted, ""];
  const links = await linksList().catch(() => []);
  for (const l of links) {
    const other = l.a.id === ctx.req.from ? l.b : l.b.id === ctx.req.from ? l.a : null;
    if (!other) continue;
    const node = sliceOf(other.scene)?.nodes.find((n) => n.id === other.id);
    const label = node ? labelOf(node) : other.label;
    if (label.toLowerCase() === namePart && (!scenePart || other.scene.toLowerCase() === scenePart)) {
      return { id: other.id, label, scene: other.scene };
    }
  }
  return null;
}

async function dismiss(ctx: Ctx): Promise<string> {
  const store = useCanvasStore.getState();
  const sl = ctx.sl;
  const { agents } = reachOf(sl.nodes, sl.edges, ctx.req.from);
  const ref = str(ctx.params.name);
  const target = findByName(agents, ref, "agent");
  if (typeof target === "string") {
    // A teammate recruited into another workspace
    const remote = await remoteTeammate(ctx, ref);
    if (!remote) return target;
    if (!sliceOf(remote.scene)) await store.wakeScene(remote.scene);
    store.removeNode(remote.id);
    return `ok: dismissed '${remote.label} @ ${remote.scene}'`;
  }
  // Notes wired only to this agent lose their last link (only the user can rewire them)
  const orphans = neighbors(sl.edges, target.id)
    .map((id) => sl.nodes.find((n) => n.id === id))
    .filter((n): n is AppNode => n?.type === "note" && neighbors(sl.edges, n.id).length === 1)
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
    const { agents } = reachOf(ctx.sl.nodes, ctx.sl.edges, ctx.req.from);
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
      // Roles are global: every live workspace's terminals count
      const all = [store.nodes, ...Object.values(store.background).map((b) => b.nodes)].flat();
      const users = all.filter((n) => n.type === "terminal" && (n.data as TerminalNodeData).roleId === role.id);
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

function floorList(ctx: Ctx): string {
  const { floors, nodes } = ctx.sl;
  const ws = useWorkspacesStore.getState();
  const count = (id: string) => nodes.filter((n) => floorOf(n) === id).length;
  const mine = floorOf(ctx.me);
  const here = (id: string) => (id === mine ? "  ← you" : "");
  const lines = [`Ground  ${ws.dirOf(ws.current) ?? "(no directory)"}  (${count(GROUND)} nodes)${here(GROUND)}`];
  for (const f of floors) {
    const pr = f.pull_request ? `PR #${f.pull_request.number}${f.pull_request.same_repo ? "" : " (fork copy)"}  ` : "";
    const iso = f.branch ? `${pr}branch ${f.branch}  ${f.copy ? "copy " : ""}${f.dir}` : "plain (shares the ground directory)";
    lines.push(`${f.name}  ${iso}  (${count(f.id)} nodes)${here(f.id)}`);
  }
  // Status lines under each level that has one
  const levels = [GROUND, ...floors.map((f) => f.id)];
  return lines
    .map((l, i) => (statusOf(levels[i], ctx.scene) ? `${l}\n    ${describeStatus(statusOf(levels[i], ctx.scene))}` : l))
    .join("\n");
}

async function floorCreate(ctx: Ctx): Promise<string> {
  const pullRequest = str(ctx.params.pull_request).trim();
  if (pullRequest && (str(ctx.params.branch) || ctx.params.existing_branch === true || ctx.params.no_git === true)) {
    return "Error: --branch, --existing-branch and --no-git don't apply with --pull-request";
  }
  try {
    const { name, summary } = await createFloor({
      name: str(ctx.params.name),
      branch: str(ctx.params.branch) || undefined,
      existingBranch: ctx.params.existing_branch === true,
      noGit: ctx.params.no_git === true,
      copyGround: ctx.params.copy_ground === true,
      copy: ctx.params.copy === true,
      pullRequest: pullRequest || undefined,
    }, ctx.scene);
    const isolated = fresh(ctx).floors.find((f) => f.name === name)?.branch;
    return (
      `ok: ${summary}. Staff it with \`narrater recruit "Name" --floor "${name}"\`` +
      (isolated ? "; land or delete it only when the user asks." : ".")
    );
  } catch (e) {
    return `Error: ${e instanceof Error ? e.message : e}`;
  }
}

/** The floor named in `name` (not Ground), for land/delete from the ground level. */
function groundOnlyTarget(ctx: Ctx, verb: string): { id: string; name: string } | string {
  if (floorOf(ctx.me) !== GROUND) {
    return `Error: a floor is ${verb === "land" ? "landed" : "deleted"} from the ground level — run it from a terminal on Ground, not from a floor`;
  }
  const ref = str(ctx.params.name).trim();
  if (!ref) return `Usage: narrater floor ${verb} "Name"`;
  const f = findFloor(ref, ctx.scene);
  if (!f) return `Error: no floor named '${ref}' — \`narrater floor list\``;
  if (f === "ground") return `Error: the ground level can't be ${verb === "land" ? "landed" : "deleted"}`;
  return { id: f.id, name: f.name };
}

async function floorLandCmd(ctx: Ctx): Promise<string> {
  const target = groundOnlyTarget(ctx, "land");
  if (typeof target === "string") return target;
  try {
    const summary = await landFloor(target.id, {
      into: str(ctx.params.into).trim() || undefined,
      deleteBranch: ctx.params.delete_branch === true,
      keepFloor: ctx.params.keep_floor === true,
    });
    return `ok: landed '${target.name}' — ${summary}`;
  } catch (e) {
    return `Error: ${e instanceof Error ? e.message : e}`;
  }
}

async function floorDeleteCmd(ctx: Ctx): Promise<string> {
  const target = groundOnlyTarget(ctx, "delete");
  if (typeof target === "string") return target;
  try {
    const summary = await deleteFloorFully(target.id, ctx.params.keep_branch === true, ctx.params.force === true);
    return `ok: deleted floor '${target.name}' and everything on it — ${summary}`;
  } catch (e) {
    return `Error: ${e instanceof Error ? e.message : e}`;
  }
}

/** `narrater floor status` — any terminal, for its own floor; --floor is the Maestro's. */
function floorStatusCmd(ctx: Ctx): string {
  const p = ctx.params;
  let id = floorOf(ctx.me);
  const floorRef = str(p.floor).trim();
  if (floorRef) {
    if (!ctx.meData.maestro) {
      return "Error: only the Maestro can post another floor's status — drop --floor to post your own floor's";
    }
    const f = findFloor(floorRef, ctx.scene);
    if (!f) return `Error: no floor named '${floorRef}' — \`narrater floor list\``;
    id = f === "ground" ? GROUND : f.id;
  }
  const text = str(p.text).trim();
  const stateRef = str(p.state).trim().toLowerCase();
  const progressRef = str(p.progress).trim();

  if (p.clear === true) {
    useCanvasStore.getState().setFloorStatus(id, null, ctx.scene);
    return `ok: cleared the status of ${floorName(id)}`;
  }
  if (!text && !stateRef && !progressRef) return `${floorName(id)}: ${describeStatus(statusOf(id, ctx.scene))}`;

  if (stateRef && !FLOOR_STATES.includes(stateRef as FloorState)) {
    return `Error: --state takes ${FLOOR_STATES.join(", ")}`;
  }
  const progress = progressRef ? parseProgress(progressRef) : undefined;
  if (progress === null) return "Error: --progress takes 2/5, 40% or 0.4";
  const line = postStatus(
    id,
    { text: text || undefined, state: (stateRef || undefined) as FloorState | undefined, progress },
    labelOf(ctx.me),
    ctx.scene,
  );
  return `ok: ${floorName(id)} ${describeStatus(line)}`;
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
  const scene = sceneOfNode(req.from);
  const sl = sliceOf(scene);
  const me = sl?.nodes.find((n) => n.id === req.from);
  if (!sl || !scene || me?.type !== "terminal") return "Error: unknown terminal";
  const meData = me.data as TerminalNodeData;
  const ctx: Ctx = { req, params: req.params ?? {}, me, meData, scene, sl };
  if (req.action === "floor_status") return floorStatusCmd(ctx);
  if (!meData.maestro) return NOT_MAESTRO;

  switch (req.action) {
    case "recruit":
      return recruit(ctx);
    case "dismiss":
      return dismiss(ctx);
    case "preset_list":
      return presetList();
    case "floor_create":
      return floorCreate(ctx);
    case "floor_list":
      return floorList(ctx);
    case "floor_land":
      return floorLandCmd(ctx);
    case "floor_delete":
      return floorDeleteCmd(ctx);
    case "notify":
      return notify(ctx);
    default:
      return roleCommand(ctx);
  }
}
