import { useEffect, useRef, useState } from "react";
import { MessagesSquare, X } from "lucide-react";
import { narraterLedger } from "@/lib/tauri";
import type { LedgerEntry } from "@/lib/tauri";
import { pairKey, useLedgerStore } from "@/stores/ledger";
import { useCanvasStore } from "@/stores/canvas";

// History panel for an agent↔agent conversation, opened by clicking an
// agent-pipe edge. Loads the ledger from the backend and reloads on each new
// message for the pair (lastActivity), with scroll pinned to the end like a chat.

const KIND_STYLE: Record<string, { color: string; label: string }> = {
  send: { color: "#a78bfa", label: "send" },
  ask: { color: "#60a5fa", label: "ask" },
  reply: { color: "#4ade80", label: "reply" },
  broadcast: { color: "#fbbf24", label: "broadcast" },
};

function timeOf(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, { hour12: false });
}

export default function EdgeHistoryPanel() {
  const openEdge = useLedgerStore((s) => s.openEdge);
  const setOpenEdge = useLedgerStore((s) => s.setOpenEdge);
  const activity = useLedgerStore((s) =>
    openEdge ? s.lastActivity[pairKey(openEdge.source, openEdge.target)] ?? 0 : 0
  );
  const [entries, setEntries] = useState<LedgerEntry[]>([]);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!openEdge) return;
    narraterLedger(openEdge.source, openEdge.target).then(setEntries).catch(console.error);
  }, [openEdge, activity]);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [entries]);

  if (!openEdge) return null;

  const labelFor = (nodeId: string) => {
    const node = useCanvasStore.getState().nodes.find((n) => n.id === nodeId);
    return (node?.data as { label?: string } | undefined)?.label ?? nodeId;
  };

  return (
    <div className="absolute top-0 right-0 h-full w-[340px] z-40 flex flex-col bg-[#141414] border-l border-[#2a2a2a] shadow-2xl">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-[#2a2a2a] shrink-0">
        <MessagesSquare size={13} className="text-[#8b5cf6] shrink-0" />
        <span className="text-xs text-[#ccc] font-medium truncate flex-1">
          {labelFor(openEdge.source)} ⇄ {labelFor(openEdge.target)}
        </span>
        <span className="text-[9px] text-[#555] shrink-0">{entries.length} msg</span>
        <button
          onClick={() => setOpenEdge(null)}
          className="text-[#555] hover:text-[#f87171] transition-colors p-0.5 rounded hover:bg-[#2a2a2a] shrink-0"
          title="Close history"
        >
          <X size={13} />
        </button>
      </div>

      {/* Messages */}
      <div ref={listRef} className="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-2">
        {entries.length === 0 && (
          <div className="text-[10px] text-[#555] text-center pt-8">
            No messages between these agents yet.
            <br />
            (the history records send, ask, reply and broadcast)
          </div>
        )}
        {entries.map((e, i) => {
          const kind = KIND_STYLE[e.kind] ?? KIND_STYLE.send;
          return (
            <div key={`${e.ts}-${i}`} className="rounded-lg bg-[#1a1a1a] border border-[#242424] px-2.5 py-1.5">
              <div className="flex items-center gap-1.5 mb-0.5">
                <span className="text-[10px] font-medium text-[#ddd] truncate">{e.from_label}</span>
                <span className="text-[9px] text-[#555]">→ {e.to_label}</span>
                <span
                  className="text-[8px] font-medium px-1 py-px rounded-full shrink-0"
                  style={{ color: kind.color, background: `${kind.color}18`, border: `1px solid ${kind.color}30` }}
                >
                  {kind.label}
                  {e.msg_id ? ` #${e.msg_id}` : ""}
                </span>
                <span className="text-[8px] text-[#444] ml-auto shrink-0">{timeOf(e.ts)}</span>
              </div>
              <div className="text-[10px] text-[#aaa] whitespace-pre-wrap break-words max-h-32 overflow-y-auto">
                {e.msg}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
