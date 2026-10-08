import { useEffect, useState } from "react";
import { Container, Laptop, Loader2, RefreshCw, Server, X } from "lucide-react";
import { useWorkspacesStore } from "@/stores/workspaces";
import { restartSceneTerminals } from "@/stores/canvas";
import { toast } from "@/stores/toasts";
import {
  environmentDockerContainers, environmentDockerContexts, environmentEndSessions, environmentSessions,
  environmentSshHosts, environmentTest, workspaceSetEnvironment,
  type ContainerInfo, type Environment,
} from "@/lib/tauri";

// Where a workspace's agents run (Maestri's SSH workspaces and Docker
// containers): this machine, an SSH server (key-based OpenSSH) or a running
// Docker container (docker exec). The agents, their roles, connections and
// the narrater CLI go along.

type Kind = Environment["kind"];

const input =
  "w-full bg-[#111] border border-[#2a2a2a] rounded px-2 py-1 text-xs text-[#ddd] focus:outline-none focus:border-[#8b5cf6]";

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[10px] uppercase tracking-wider text-[#666]">{label}</span>
      {children}
      {hint && <span className="text-[10px] text-[#555]">{hint}</span>}
    </label>
  );
}

export default function EnvironmentSheet({ scene, onClose }: { scene: string; onClose: () => void }) {
  const meta = useWorkspacesStore((s) => s.layout.scenes[scene]);
  const loadLayout = useWorkspacesStore((s) => s.loadLayout);
  const current = meta?.environment ?? { kind: "local" as const };

  const [kind, setKind] = useState<Kind>(current.kind);
  const [host, setHost] = useState(current.kind === "ssh" ? current.host : "");
  const [port, setPort] = useState(current.kind === "ssh" && current.port ? String(current.port) : "22");
  const [sshUser, setSshUser] = useState(current.kind === "ssh" ? current.user ?? "" : "");
  const [container, setContainer] = useState(current.kind === "docker" ? current.container : "");
  const [context, setContext] = useState(current.kind === "docker" ? current.context ?? "" : "");
  const [dockerUser, setDockerUser] = useState(current.kind === "docker" ? current.user ?? "" : "");
  const [shell, setShell] = useState(current.kind === "docker" ? current.shell ?? "" : "");
  const [persistent, setPersistent] = useState(current.kind !== "local" && !!current.persistent);
  const [dir, setDir] = useState(meta?.dir ?? "");

  const [hosts, setHosts] = useState<string[]>([]);
  const [contexts, setContexts] = useState<string[]>([]);
  const [containers, setContainers] = useState<ContainerInfo[] | null>(null);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [sessions, setSessions] = useState<string[] | null>(null);

  useEffect(() => {
    environmentSshHosts().then(setHosts).catch(() => {});
    environmentDockerContexts().then(setContexts).catch(() => {});
  }, []);

  const refreshContainers = () => {
    setContainers(null);
    environmentDockerContainers(context || null)
      .then(setContainers)
      .catch((e) => {
        setContainers([]);
        setResult({ ok: false, text: String(e) });
      });
  };
  useEffect(() => {
    if (kind === "docker") refreshContainers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind, context]);

  const build = (): Environment => {
    if (kind === "ssh") {
      const p = Number(port);
      return { kind, host: host.trim(), port: p && p !== 22 ? p : null, user: sshUser.trim() || null, persistent };
    }
    if (kind === "docker") {
      return {
        kind, container: container.trim(), context: context || null, user: dockerUser.trim() || null,
        shell: shell.trim() || null, persistent,
      };
    }
    return { kind: "local" };
  };

  // Persistent sessions of the saved environment
  const savedRemote = current.kind !== "local" ? current : null;
  const loadSessions = () => {
    if (!savedRemote) return;
    environmentSessions(savedRemote).then(setSessions).catch(() => setSessions([]));
  };
  useEffect(loadSessions, [scene]); // eslint-disable-line react-hooks/exhaustive-deps

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult({ ok: true, text: await environmentTest(build()) });
    } catch (e) {
      setResult({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const env = build();
      const before = { env: JSON.stringify(current), dir: meta?.dir ?? null };
      const newDir = dir.trim() || null;
      await workspaceSetEnvironment(scene, env, newDir);
      await loadLayout();
      // Moved: its terminals start again over there, in the new directory
      if (before.env !== JSON.stringify(env) || before.dir !== newDir) {
        restartSceneTerminals(scene, { from: before.dir, to: newDir });
      }
      toast.success(env.kind === "local" ? `'${scene}' runs on this machine` : `'${scene}' now runs on ${describe(env)}`);
      onClose();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  const tab = (k: Kind, icon: React.ReactNode, label: string) => (
    <button
      onClick={() => { setKind(k); setResult(null); }}
      className={`flex-1 flex items-center justify-center gap-1.5 py-1.5 rounded-md text-xs ${
        kind === k ? "bg-[#8b5cf633] text-white" : "text-[#777] hover:text-[#bbb]"
      }`}
    >
      {icon} {label}
    </button>
  );

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50" onMouseDown={onClose}>
      <div
        className="w-[440px] max-w-[92vw] rounded-xl bg-[#141414] border border-[#2a2a2a] shadow-2xl p-4 flex flex-col gap-3"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <div className="text-sm text-white">Where “{scene}” runs</div>
          <button onClick={onClose} className="text-[#555] hover:text-white"><X size={14} /></button>
        </div>

        <div className="flex gap-1 p-1 rounded-lg bg-[#0e0e0e] border border-[#222]">
          {tab("local", <Laptop size={12} />, "This machine")}
          {tab("ssh", <Server size={12} />, "SSH server")}
          {tab("docker", <Container size={12} />, "Docker")}
        </div>

        {kind === "ssh" && (
          <>
            <div className="grid grid-cols-[1fr_70px] gap-2">
              <Field label="Host" hint={hosts.length ? "Hosts from ~/.ssh/config are suggested" : undefined}>
                <input list="narrater-ssh-hosts" value={host} onChange={(e) => setHost(e.target.value)} placeholder="build-box" className={input} />
                <datalist id="narrater-ssh-hosts">{hosts.map((h) => <option key={h} value={h} />)}</datalist>
              </Field>
              <Field label="Port">
                <input value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))} className={input} />
              </Field>
            </div>
            <Field label="User" hint="Key-based login only (keys in ~/.ssh) — password prompts are off">
              <input value={sshUser} onChange={(e) => setSshUser(e.target.value)} placeholder="(from ~/.ssh/config)" className={input} />
            </Field>
          </>
        )}

        {kind === "docker" && (
          <>
            <div className="grid grid-cols-2 gap-2">
              <Field label="Context">
                <select value={context} onChange={(e) => setContext(e.target.value)} className={input}>
                  <option value="">(current)</option>
                  {contexts.map((c) => <option key={c} value={c}>{c}</option>)}
                </select>
              </Field>
              <Field label="Container">
                <div className="flex gap-1">
                  <select value={container} onChange={(e) => setContainer(e.target.value)} className={input}>
                    <option value="">{containers === null ? "loading…" : containers.length ? "pick one" : "none running"}</option>
                    {containers?.map((c) => <option key={c.name} value={c.name}>{c.name} · {c.image}</option>)}
                    {container && !containers?.some((c) => c.name === container) && <option value={container}>{container}</option>}
                  </select>
                  <button onClick={refreshContainers} title="Refresh" className="text-[#666] hover:text-white px-1"><RefreshCw size={11} /></button>
                </div>
              </Field>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <Field label="User"><input value={dockerUser} onChange={(e) => setDockerUser(e.target.value)} placeholder="(image default)" className={input} /></Field>
              <Field label="Shell"><input value={shell} onChange={(e) => setShell(e.target.value)} placeholder="/bin/sh" className={input} /></Field>
            </div>
            <div className="text-[10px] text-[#555]">
              NarraTer never creates, starts or stops the container, and adds no mounts — the project must already be inside it.
            </div>
          </>
        )}

        {kind !== "local" && (
          <>
            <Field label="Project directory" hint={kind === "ssh" ? "A path on the server (~ is its home)" : "A path inside the container"}>
              <input value={dir} onChange={(e) => setDir(e.target.value)} placeholder="~/app" className={input} />
            </Field>
            <label className="flex items-start gap-2 text-xs text-[#bbb] select-none">
              <input type="checkbox" checked={persistent} onChange={(e) => setPersistent(e.target.checked)} className="mt-0.5 accent-[#8b5cf6]" />
              <span>
                Persistent session
                <span className="block text-[10px] text-[#555]">Agents keep running in tmux when NarraTer quits or the connection drops; reconnecting reattaches.</span>
              </span>
            </label>
          </>
        )}
        {kind === "local" && (
          <Field label="Project directory">
            <input value={dir} onChange={(e) => setDir(e.target.value)} placeholder="~/Projects/app" className={input} />
          </Field>
        )}

        {result && (
          <div className={`text-[11px] rounded-md px-2 py-1.5 break-words ${result.ok ? "bg-[#14532d55] text-[#86efac]" : "bg-[#7f1d1d55] text-[#fca5a5]"}`}>
            {result.text}
          </div>
        )}

        {savedRemote?.persistent && sessions && sessions.length > 0 && (
          <div className="flex flex-col gap-1">
            <div className="text-[10px] uppercase tracking-wider text-[#666]">Persistent sessions running there</div>
            {sessions.map((s) => (
              <div key={s} className="flex items-center justify-between text-[11px] text-[#999]">
                <span className="font-mono">{s}</span>
                <button
                  onClick={() => environmentEndSessions(savedRemote, s).then(loadSessions).catch((e) => toast.error(String(e)))}
                  className="text-[#f87171] hover:text-white"
                >
                  End
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="flex items-center justify-end gap-2 pt-1">
          {kind !== "local" && (
            <button onClick={test} disabled={testing} className="flex items-center gap-1 px-3 py-1 rounded-md text-xs text-[#ccc] border border-[#333] hover:border-[#555]">
              {testing && <Loader2 size={11} className="animate-spin" />} Test
            </button>
          )}
          <button onClick={save} disabled={saving} className="px-3 py-1 rounded-md text-xs text-white bg-[#8b5cf6] hover:bg-[#7c3aed] disabled:opacity-50">
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

function describe(env: Environment): string {
  if (env.kind === "ssh") return `${env.user ? `${env.user}@` : ""}${env.host}`;
  if (env.kind === "docker") return `container ${env.container}`;
  return "this machine";
}
