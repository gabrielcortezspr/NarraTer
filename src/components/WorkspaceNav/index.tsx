import { useEffect, useRef, useState } from "react";
import { useReactFlow } from "@xyflow/react";
import { GROUND, floorOf, useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { focusTerminal } from "@/lib/terminalManager";
import {
  cycleWorkspace, gotoWorkspace, lastWorkspace, orderedWorkspaces, recentWorkspaces, switchWorkspace,
} from "@/lib/workspaceNav";

// tmux-style keyboard navigation (Maestri Pro): a prefix (Ctrl+Space) then a
// key jumps between workspaces, terminals and floors; Ctrl+Tab cycles recent
// workspaces like an app switcher. Captured before xterm sees the keys, so it
// works with the focus inside a terminal.

const PREFIX_TIMEOUT_MS = 2000;

export const NAV_SHORTCUTS: Array<[string, string]> = [
  ["Ctrl+Tab", "Recent workspaces (hold Ctrl, tap Tab, release)"],
  ["Ctrl+Space  n / p", "Next / previous workspace"],
  ["Ctrl+Space  1…9", "Workspace by position in the sidebar"],
  ["Ctrl+Space  l", "Last workspace"],
  ["Ctrl+Space  w", "Workspace switcher"],
  ["Ctrl+Space  o / O", "Next / previous terminal (focuses it)"],
  ["Ctrl+Space  f / F", "Next / previous floor"],
  ["Ctrl+Space  ?", "This list"],
];

type Overlay =
  | { kind: "recent"; items: string[]; index: number }
  | { kind: "picker"; query: string; index: number }
  | { kind: "help" };

const isPrefix = (e: KeyboardEvent) => e.ctrlKey && !e.altKey && !e.metaKey && (e.code === "Space" || e.key === " ");

export default function WorkspaceNav() {
  const { fitView } = useReactFlow();
  const [armed, setArmed] = useState(false);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const running = useCanvasStore((s) => Object.keys(s.background).sort().join("\n"));
  const current = useWorkspacesStore((s) => s.current);
  const armTimer = useRef<number | undefined>(undefined);
  const overlayRef = useRef(overlay);
  overlayRef.current = overlay;
  const armedRef = useRef(armed);
  armedRef.current = armed;

  useEffect(() => {
    const disarm = () => {
      window.clearTimeout(armTimer.current);
      setArmed(false);
    };

    /** Next/previous terminal on the floor on screen, left-to-right, top-to-bottom. */
    const cycleTerminal = (delta: number) => {
      const { nodes, currentFloor, onNodesChange } = useCanvasStore.getState();
      const terms = nodes
        .filter((n) => n.type === "terminal" && floorOf(n) === currentFloor)
        .sort((a, b) => a.position.x - b.position.x || a.position.y - b.position.y);
      if (!terms.length) return;
      const focused = (document.activeElement as HTMLElement | null)?.closest?.(".react-flow__node")?.getAttribute("data-id");
      const at = terms.findIndex((n) => n.id === (focused ?? nodes.find((x) => x.selected)?.id));
      const next = terms[at < 0 ? 0 : (at + delta + terms.length) % terms.length];
      onNodesChange(nodes.map((n) => ({ id: n.id, type: "select" as const, selected: n.id === next.id })));
      fitView({ nodes: [{ id: next.id }], maxZoom: 1.1, duration: 200 });
      window.setTimeout(() => focusTerminal(next.id), 220);
    };

    const cycleFloor = (delta: number) => {
      const { floors, currentFloor, setFloor } = useCanvasStore.getState();
      const levels = [GROUND, ...floors.map((f) => f.id)];
      if (levels.length < 2) return;
      const i = Math.max(0, levels.indexOf(currentFloor));
      setFloor(levels[(i + delta + levels.length) % levels.length]);
    };

    const afterPrefix = (e: KeyboardEvent) => {
      const k = e.key;
      if (k === "n") void cycleWorkspace(1);
      else if (k === "p") void cycleWorkspace(-1);
      else if (k === "l") void lastWorkspace();
      else if (/^[1-9]$/.test(k)) void gotoWorkspace(Number(k));
      else if (k === "w") setOverlay({ kind: "picker", query: "", index: 0 });
      else if (k === "o") cycleTerminal(1);
      else if (k === "O") cycleTerminal(-1);
      else if (k === "f") cycleFloor(1);
      else if (k === "F") cycleFloor(-1);
      else if (k === "?") setOverlay({ kind: "help" });
    };

    const onKeyDown = (e: KeyboardEvent) => {
      // Ctrl+Tab: app-switcher style over recent workspaces
      if (e.ctrlKey && e.key === "Tab") {
        e.preventDefault();
        e.stopPropagation();
        const o = overlayRef.current;
        const step = e.shiftKey ? -1 : 1;
        if (o?.kind === "recent") {
          setOverlay({ ...o, index: (o.index + step + o.items.length) % o.items.length });
        } else {
          const items = recentWorkspaces();
          if (items.length > 1) setOverlay({ kind: "recent", items, index: (step + items.length) % items.length });
        }
        return;
      }
      if (isPrefix(e)) {
        e.preventDefault();
        e.stopPropagation();
        setArmed(true);
        window.clearTimeout(armTimer.current);
        armTimer.current = window.setTimeout(() => setArmed(false), PREFIX_TIMEOUT_MS);
        return;
      }
      if (armedRef.current) {
        // Modifiers alone (Shift for "O", "?") don't consume the prefix
        if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
        e.preventDefault();
        e.stopPropagation();
        disarm();
        if (e.key !== "Escape") afterPrefix(e);
        return;
      }
      const o = overlayRef.current;
      if (!o) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setOverlay(null);
      } else if (o.kind === "help") {
        setOverlay(null);
      }
    };

    // Releasing Ctrl ends a Ctrl+Tab cycle on the highlighted workspace
    const onKeyUp = (e: KeyboardEvent) => {
      const o = overlayRef.current;
      if (o?.kind === "recent" && e.key === "Control") {
        setOverlay(null);
        void switchWorkspace(o.items[o.index]);
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.clearTimeout(armTimer.current);
    };
  }, [fitView]);

  const runningSet = new Set(running ? running.split("\n") : []);
  const dot = (name: string) => (
    <span
      className="w-1.5 h-1.5 rounded-full shrink-0"
      style={{ background: name === current ? "#8b5cf6" : runningSet.has(name) ? "#4ade80" : "#333" }}
    />
  );

  return (
    <>
      {armed && (
        <div className="absolute bottom-16 left-1/2 -translate-x-1/2 z-50 px-3 py-1.5 rounded-md bg-[#1a1a1a] border border-[#8b5cf6] text-[11px] text-[#c4b5fd] shadow-xl pointer-events-none">
          Ctrl+Space — n/p workspace · 1…9 · l last · w switcher · o terminal · f floor · ? help
        </div>
      )}
      {overlay?.kind === "recent" && (
        <div className="absolute inset-0 z-50 flex items-center justify-center pointer-events-none">
          <div className="min-w-[260px] max-w-[420px] rounded-xl bg-[#141414]/95 border border-[#2a2a2a] shadow-2xl p-2">
            {overlay.items.map((name, i) => (
              <div
                key={name}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-md text-xs ${
                  i === overlay.index ? "bg-[#8b5cf633] text-white" : "text-[#888]"
                }`}
              >
                {dot(name)}
                <span className="truncate">{name}</span>
                {i === 0 && <span className="ml-auto text-[9px] text-[#555]">current</span>}
              </div>
            ))}
          </div>
        </div>
      )}
      {overlay?.kind === "picker" && (
        <Picker
          query={overlay.query}
          index={overlay.index}
          dot={dot}
          onChange={(query, index) => setOverlay({ kind: "picker", query, index })}
          onPick={(name) => {
            setOverlay(null);
            void switchWorkspace(name);
          }}
          onClose={() => setOverlay(null)}
        />
      )}
      {overlay?.kind === "help" && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/40" onMouseDown={() => setOverlay(null)}>
          <div className="w-[420px] rounded-xl bg-[#141414] border border-[#2a2a2a] shadow-2xl p-4">
            <div className="text-[11px] uppercase tracking-wider text-[#666] mb-3">Navigation</div>
            {NAV_SHORTCUTS.map(([keys, what]) => (
              <div key={keys} className="flex items-center justify-between gap-4 py-0.5">
                <span className="text-[11px] font-mono text-[#8b5cf6] whitespace-pre">{keys}</span>
                <span className="text-[11px] text-[#999] text-right">{what}</span>
              </div>
            ))}
            <div className="mt-3 text-[10px] text-[#555]">
              Workspaces you leave keep running in the background (green dot); right-click one in the sidebar to stop it.
            </div>
          </div>
        </div>
      )}
    </>
  );
}

function Picker({
  query, index, dot, onChange, onPick, onClose,
}: {
  query: string;
  index: number;
  dot: (name: string) => React.ReactNode;
  onChange: (query: string, index: number) => void;
  onPick: (name: string) => void;
  onClose: () => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    requestAnimationFrame(() => inputRef.current?.focus());
  }, []);
  const all = orderedWorkspaces();
  const q = query.trim().toLowerCase();
  const items = q ? all.filter((n) => n.toLowerCase().includes(q)) : all;
  const sel = Math.min(index, Math.max(items.length - 1, 0));
  return (
    <div className="absolute inset-0 z-50 flex items-start justify-center pt-[18vh] bg-black/40" onMouseDown={onClose}>
      <div className="w-[360px] rounded-xl bg-[#141414] border border-[#2a2a2a] shadow-2xl overflow-hidden" onMouseDown={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => onChange(e.target.value, 0)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") { e.preventDefault(); onChange(query, Math.min(sel + 1, items.length - 1)); }
            else if (e.key === "ArrowUp") { e.preventDefault(); onChange(query, Math.max(sel - 1, 0)); }
            else if (e.key === "Enter" && items[sel]) onPick(items[sel]);
            else if (e.key === "Escape") onClose();
          }}
          placeholder="Switch to workspace…"
          className="w-full bg-transparent px-3 py-2.5 text-sm text-white border-b border-[#2a2a2a] focus:outline-none"
        />
        <div className="max-h-[50vh] overflow-y-auto p-1">
          {items.map((name, i) => (
            <button
              key={name}
              onClick={() => onPick(name)}
              className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-xs text-left ${
                i === sel ? "bg-[#8b5cf633] text-white" : "text-[#888] hover:bg-[#1f1f1f]"
              }`}
            >
              {dot(name)}
              <span className="truncate">{name}</span>
              {i < 9 && !q && <span className="ml-auto text-[9px] font-mono text-[#555]">{i + 1}</span>}
            </button>
          ))}
          {items.length === 0 && <div className="px-3 py-2 text-xs text-[#555]">No workspace matches</div>}
        </div>
      </div>
    </div>
  );
}
