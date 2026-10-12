import { useEffect, useMemo, useRef, useState } from "react";
import { AtSign, Globe, ImagePlus, Send, StickyNote, TerminalSquare, X } from "lucide-react";
import { useCanvasStore, type AppNode, type TerminalNodeData } from "@/stores/canvas";
import { useComposerStore } from "@/stores/composer";
import { toast } from "@/stores/toasts";
import { labelOf, reachOf } from "@/lib/canvasReach";
import { composerAttach, composerPasteImage, composerSend, linksList } from "@/lib/tauri";

// The prompt composer (Maestri's floating composer): write a prompt for a
// terminal outside it — multi-line, `@` to reference what it can reach
// (agents, notes, portals, teammates in other workspaces), pasted
// screenshots — and send it with Ctrl+Enter; it's typed in when the agent
// is idle. Each terminal keeps its draft. Ctrl+Shift+P opens it for the
// focused or selected terminal.

interface Mention {
  name: string;
  kind: "agent" | "note" | "portal";
  /** How the agent uses it */
  hint: string;
}

function mentionText(name: string): string {
  return /\s/.test(name) ? `@"${name}"` : `@${name}`;
}

/** The terminal under the keyboard focus, else the selected one. */
export function composerTargetFromFocus(): string | null {
  const { nodes } = useCanvasStore.getState();
  const focused = (document.activeElement as HTMLElement | null)?.closest?.(".react-flow__node")?.getAttribute("data-id");
  const isTerminal = (id?: string | null) => !!id && nodes.some((n) => n.id === id && n.type === "terminal");
  if (isTerminal(focused)) return focused!;
  return nodes.find((n) => n.selected && n.type === "terminal")?.id ?? null;
}

export default function Composer() {
  const target = useComposerStore((s) => s.target);
  const drafts = useComposerStore((s) => s.drafts);
  const { open, close, setDraft } = useComposerStore.getState();
  const nodes = useCanvasStore((s) => s.nodes);
  const edges = useCanvasStore((s) => s.edges);
  const node = nodes.find((n) => n.id === target && n.type === "terminal");
  const data = node?.data as TerminalNodeData | undefined;
  const text = (target && drafts[target]) || "";
  const isShell = data?.agentType === "shell";

  const areaRef = useRef<HTMLTextAreaElement>(null);
  const [remote, setRemote] = useState<Mention[]>([]);
  const [popup, setPopup] = useState<{ query: string; start: number; index: number } | null>(null);
  const [busy, setBusy] = useState(false);

  // Ctrl+Shift+P: open for the focused/selected terminal (works from inside xterm)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey && e.shiftKey && !e.altKey && e.key.toLowerCase() === "p")) return;
      e.preventDefault();
      e.stopPropagation();
      if (useComposerStore.getState().target) {
        close();
        return;
      }
      const id = composerTargetFromFocus();
      if (id) open(id);
      else toast.warning("Select or click into a terminal to write a prompt for it");
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, close]);

  useEffect(() => {
    if (!target) return;
    requestAnimationFrame(() => {
      const el = areaRef.current;
      if (!el) return;
      el.focus();
      el.selectionStart = el.selectionEnd = el.value.length;
    });
    // Teammates in other workspaces, linked to this terminal
    linksList()
      .then((links) =>
        setRemote(
          links.flatMap((l) => {
            const other = l.a.id === target ? l.b : l.b.id === target ? l.a : null;
            if (!other) return [];
            const name = `${other.label} @ ${other.scene}`;
            return [{ name, kind: "agent" as const, hint: `narrater ask "${name}" "…"` }];
          }),
        ),
      )
      .catch(() => setRemote([]));
  }, [target]);

  // What this terminal can reach, as mentionable names
  const mentions = useMemo<Mention[]>(() => {
    if (!target) return [];
    const { agents, notes, portals } = reachOf(nodes, edges, target);
    const local: Mention[] = [
      ...agents.map((a: AppNode) => ({ name: labelOf(a), kind: "agent" as const, hint: `narrater ask "${labelOf(a)}" "…"` })),
      ...notes.map((n) => ({ name: labelOf(n.node), kind: "note" as const, hint: `narrater note read "${labelOf(n.node)}"` })),
      ...portals.map((p: AppNode) => ({ name: labelOf(p), kind: "portal" as const, hint: `narrater portal snapshot "${labelOf(p)}"` })),
    ];
    return [...local, ...remote];
  }, [target, nodes, edges, remote]);

  if (!target || !node || !data) return null;

  const matches = popup
    ? mentions.filter((m) => m.name.toLowerCase().includes(popup.query.toLowerCase())).slice(0, 8)
    : [];

  const updatePopup = (value: string, caret: number) => {
    const before = value.slice(0, caret);
    const m = /(^|\s)@("?)([^\s@"]*)$/.exec(before);
    if (m) setPopup({ query: m[3], start: caret - m[3].length - m[2].length - 1, index: 0 });
    else setPopup(null);
  };

  const insertMention = (m: Mention) => {
    if (!popup || !areaRef.current) return;
    const caret = areaRef.current.selectionStart;
    const next = `${text.slice(0, popup.start)}${mentionText(m.name)} ${text.slice(caret)}`;
    setDraft(target, next);
    setPopup(null);
    const pos = popup.start + mentionText(m.name).length + 1;
    requestAnimationFrame(() => {
      areaRef.current?.focus();
      if (areaRef.current) areaRef.current.selectionStart = areaRef.current.selectionEnd = pos;
    });
  };

  const send = async () => {
    const body = text.trim();
    if (!body) return;
    // References become instructions the agent can act on (shells get the bare text)
    const used = mentions.filter((m) => body.includes(mentionText(m.name)));
    const footer = !isShell && used.length
      ? `\n\n(Referenced on the canvas: ${used.map((m) => `${m.kind} "${m.name}" — \`${m.hint}\``).join("; ")})`
      : "";
    setBusy(true);
    try {
      const { ahead, waiting } = await composerSend(target, body + footer);
      setDraft(target, "");
      close();
      toast.success(
        ahead
          ? `Queued for ${data.label} — ${ahead} prompt(s) ahead; sent when it's idle`
          : waiting
            ? `Queued for ${data.label} — sent when it's idle`
            : `Sent to ${data.label}`,
      );
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  // After an await: the draft may have changed meanwhile
  const insertPath = (path: string) => {
    const current = useComposerStore.getState().drafts[target] ?? "";
    const el = areaRef.current;
    const at = el ? Math.min(el.selectionStart, current.length) : current.length;
    const piece = `${at > 0 && !/\s$/.test(current.slice(0, at)) ? " " : ""}${path} `;
    setDraft(target, current.slice(0, at) + piece + current.slice(at));
    toast.success("Image attached — its path is in the prompt");
  };

  const attach = async (file: File) => {
    const ext = file.type.split("/")[1] || "png";
    const b64 = await new Promise<string>((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(",")[1] ?? "");
      r.onerror = () => reject(r.error);
      r.readAsDataURL(file);
    });
    try {
      insertPath(await composerAttach(target, b64, ext));
    } catch (e) {
      toast.error(String(e));
    }
  };

  // WebKitGTK pastes clipboard images as an empty event: read it natively
  const attachFromClipboard = async () => {
    try {
      const path = await composerPasteImage(target);
      if (path) insertPath(path);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const icon = (kind: Mention["kind"]) =>
    kind === "agent" ? <TerminalSquare size={11} /> : kind === "note" ? <StickyNote size={11} /> : <Globe size={11} />;

  return (
    <div className="absolute bottom-6 left-1/2 -translate-x-1/2 z-40 w-[640px] max-w-[92%]" onMouseDown={(e) => e.stopPropagation()}>
      <div className="relative rounded-xl bg-[#141414]/95 border border-[#2a2a2a] shadow-2xl backdrop-blur">
        <div className="flex items-center justify-between px-3 pt-2 text-[11px]">
          <span className="text-[#777]">
            Prompt for <span className="text-[#c4b5fd]">{data.label}</span>
            {isShell && <span className="text-[#555]"> · shell: sent as a command</span>}
          </span>
          <button onClick={close} title="Close (the draft stays)" className="text-[#555] hover:text-white"><X size={12} /></button>
        </div>
        <textarea
          ref={areaRef}
          value={text}
          rows={Math.min(10, Math.max(3, text.split("\n").length))}
          onChange={(e) => {
            setDraft(target, e.target.value);
            updatePopup(e.target.value, e.target.selectionStart);
          }}
          onClick={(e) => updatePopup(text, (e.target as HTMLTextAreaElement).selectionStart)}
          onKeyDown={(e) => {
            if (popup && matches.length) {
              if (e.key === "ArrowDown") { e.preventDefault(); setPopup({ ...popup, index: (popup.index + 1) % matches.length }); return; }
              if (e.key === "ArrowUp") { e.preventDefault(); setPopup({ ...popup, index: (popup.index - 1 + matches.length) % matches.length }); return; }
              if (e.key === "Enter" || e.key === "Tab") { e.preventDefault(); insertMention(matches[popup.index]); return; }
            }
            if (e.key === "Escape") {
              e.preventDefault();
              if (popup) setPopup(null);
              else close();
            } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              void send();
            }
          }}
          onPaste={(e) => {
            const file = [...e.clipboardData.files].find((f) => f.type.startsWith("image/"));
            if (file) {
              e.preventDefault();
              void attach(file);
            } else if (e.clipboardData.types.length === 0) {
              void attachFromClipboard();
            }
          }}
          placeholder={isShell ? "A command (multi-line ok)…" : "Write a prompt… @ to reference a teammate, note or portal; paste a screenshot"}
          className="w-full bg-transparent px-3 py-2 text-[13px] text-[#e5e5e5] resize-none focus:outline-none font-mono leading-relaxed"
        />
        {popup && matches.length > 0 && (
          <div className="absolute bottom-full left-3 mb-1 w-[300px] rounded-lg bg-[#1a1a1a] border border-[#2a2a2a] shadow-xl p-1">
            {matches.map((m, i) => (
              <button
                key={`${m.kind}-${m.name}`}
                onMouseDown={(e) => { e.preventDefault(); insertMention(m); }}
                className={`w-full flex items-center gap-2 px-2 py-1 rounded text-xs text-left ${i === popup.index ? "bg-[#8b5cf633] text-white" : "text-[#999]"}`}
              >
                {icon(m.kind)}
                <span className="truncate">{m.name}</span>
                <span className="ml-auto text-[9px] text-[#555]">{m.kind}</span>
              </button>
            ))}
          </div>
        )}
        <div className="flex items-center justify-between px-3 pb-2 text-[10px] text-[#555]">
          <span className="flex items-center gap-3">
            <span className="flex items-center gap-1"><AtSign size={10} /> mention</span>
            <span className="flex items-center gap-1"><ImagePlus size={10} /> paste an image</span>
            <span>Esc closes · draft kept</span>
          </span>
          <button
            onClick={() => void send()}
            disabled={busy || !text.trim()}
            className="flex items-center gap-1 px-2 py-1 rounded-md bg-[#8b5cf6] text-white disabled:opacity-40"
          >
            <Send size={11} /> Send <span className="text-[#ddd6fe]">Ctrl+↵</span>
          </button>
        </div>
      </div>
    </div>
  );
}
