import { useCallback, useEffect, useRef, useState } from "react";
import { MessageSquare, X, Paperclip, Send, Trash2, FileText, Loader2 } from "lucide-react";
import { chatKey, useChatStore } from "@/stores/chat";
import { useCanvasStore } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import {
  CHAT_THREADS, chatClear, chatHistory, chatSend, chatThreads, fsReadFileBase64, openPath, pickFile,
  type ChatAttachment, type ChatEntry, type ChatThread,
} from "@/lib/tauri";
import { toast } from "@/stores/toasts";

// NarraTer Chat: talk to an agent without its terminal. Messages reach the
// agent with the chat header; it answers with `narrater say` (bubbles), which
// lands here. Seven threads per agent, by color.
const THREAD_COLOR: Record<ChatThread, string> = {
  // Apple's dark system colours (docs/design.md)
  blue: "#0a84ff", purple: "#8f7cff", pink: "#ff7eb6", red: "#ff453a",
  orange: "#ff9f0a", yellow: "#ffd60a", green: "#30d158",
};

function timeOf(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
}

/** Inline image from the attachment store (data URL, cached per path). */
const imageCache = new Map<string, string>();
function ChatImage({ a }: { a: ChatAttachment }) {
  const [src, setSrc] = useState(imageCache.get(a.path) ?? "");
  useEffect(() => {
    if (src) return;
    fsReadFileBase64(a.path)
      .then((b) => {
        const url = `data:${b.mime};base64,${b.base64}`;
        imageCache.set(a.path, url);
        setSrc(url);
      })
      .catch(() => setSrc(""));
  }, [a.path, src]);
  if (!src) return <div className="w-40 h-24 rounded bg-surface animate-pulse" />;
  return (
    <img
      src={src}
      alt={a.name}
      title={`${a.name} — click to open`}
      onClick={() => openPath(a.path).catch((e) => toast.error(String(e)))}
      className="max-w-full max-h-64 rounded-md cursor-zoom-in border border-line"
    />
  );
}

function Attachments({ items }: { items: ChatAttachment[] }) {
  if (!items.length) return null;
  return (
    <div className="mt-1.5 flex flex-col gap-1.5">
      {items.map((a) =>
        a.image ? (
          <ChatImage key={a.id} a={a} />
        ) : (
          <button
            key={a.id}
            onClick={() => openPath(a.path).catch((e) => toast.error(String(e)))}
            className="flex items-center gap-1.5 text-[10px] px-2 py-1 rounded bg-black/30 hover:bg-black/50 text-left"
            title={a.path}
          >
            <FileText size={11} /> <span className="truncate">{a.name}</span>
          </button>
        )
      )}
    </div>
  );
}

export default function ChatPanel() {
  const terminalId = useChatStore((s) => s.openTerminal);
  const close = useChatStore((s) => s.close);
  const setThread = useChatStore((s) => s.setThread);
  const thread = useChatStore((s) => (terminalId ? s.threadOf[terminalId] ?? "blue" : "blue"));
  const version = useChatStore((s) => (terminalId ? s.versions[chatKey(terminalId, thread)] ?? 0 : 0));
  const anyVersion = useChatStore((s) => s.versions);
  const label = useCanvasStore((s) => {
    const n = s.nodes.find((x) => x.id === terminalId);
    return (n?.data as { label?: string } | undefined)?.label ?? "";
  });
  const status = useTerminalsStore((s) => (terminalId ? s.sessions[terminalId]?.status : undefined));

  const [entries, setEntries] = useState<ChatEntry[]>([]);
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [text, setText] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [sending, setSending] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!terminalId) return;
    chatHistory(terminalId, thread).then(setEntries).catch(console.error);
  }, [terminalId, thread, version]);

  useEffect(() => {
    if (!terminalId) return;
    chatThreads(terminalId)
      .then((t) => setCounts(Object.fromEntries(t.map((x) => [x.thread, x.count]))))
      .catch(console.error);
  }, [terminalId, anyVersion]);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [entries]);

  useEffect(() => {
    if (terminalId) inputRef.current?.focus();
    setConfirmClear(false);
  }, [terminalId, thread]);

  const send = useCallback(async () => {
    if (!terminalId || sending || (!text.trim() && !files.length)) return;
    setSending(true);
    try {
      await chatSend(terminalId, thread, text, files);
      setText("");
      setFiles([]);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSending(false);
    }
  }, [terminalId, thread, text, files, sending]);

  if (!terminalId) return null;

  // The agent's turn is on: the last word is the user's (or a progress update)
  const last = entries[entries.length - 1];
  const working = !!last && (last.role === "user" || last.progress) && status !== "exited";

  return (
    <div className="absolute top-0 right-0 h-full w-[360px] z-40 flex flex-col bg-surface border-l border-line shadow-2xl">
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-line shrink-0">
        <MessageSquare size={13} style={{ color: THREAD_COLOR[thread] }} className="shrink-0" />
        <span className="text-xs text-ink font-medium truncate flex-1">{label}</span>
        {confirmClear ? (
          <button
            onClick={() => { setConfirmClear(false); chatClear(terminalId, thread).catch((e) => toast.error(String(e))); }}
            className="text-[9px] text-danger-text hover:text-ink px-1"
          >
            clear {thread}?
          </button>
        ) : (
          <button onClick={() => setConfirmClear(true)} title="Clear this thread" className="text-ink-faint hover:text-danger-text p-0.5">
            <Trash2 size={11} />
          </button>
        )}
        <button onClick={close} title="Close chat" className="text-ink-muted hover:text-danger-text p-0.5 rounded hover:bg-hover">
          <X size={13} />
        </button>
      </div>

      {/* Threads */}
      <div className="flex items-center gap-1.5 px-3 py-2 border-b border-line shrink-0">
        {CHAT_THREADS.map((t) => (
          <button
            key={t}
            onClick={() => setThread(terminalId, t)}
            title={`${t} thread${counts[t] ? ` · ${counts[t]} messages` : ""}`}
            className="relative w-4 h-4 rounded-full transition-transform"
            style={{
              background: counts[t] ? THREAD_COLOR[t] : "transparent",
              border: `2px solid ${THREAD_COLOR[t]}`,
              transform: t === thread ? "scale(1.25)" : undefined,
              boxShadow: t === thread ? `0 0 0 2px var(--surface), 0 0 0 3.5px ${THREAD_COLOR[t]}` : undefined,
            }}
          />
        ))}
        <span className="ml-auto text-[9px] text-ink-faint">{thread}</span>
      </div>

      {/* Messages */}
      <div ref={listRef} className="flex-1 min-h-0 overflow-y-auto px-3 py-3 space-y-2">
        {entries.length === 0 && (
          <div className="text-[10px] text-ink-muted text-center pt-10 leading-relaxed">
            Talk to <span className="text-ink-2">{label}</span> here.
            <br />
            It answers with <code className="text-ink-muted">narrater say</code>.
          </div>
        )}
        {entries.map((e) =>
          e.progress ? (
            <div key={e.id} className="text-[10px] text-ink-muted italic px-1">
              {e.text} · {timeOf(e.ts)}
            </div>
          ) : (
            <div key={e.id} className={`flex ${e.role === "user" ? "justify-end" : "justify-start"}`}>
              <div
                className="max-w-[85%] rounded-xl px-3 py-2 text-[11px] leading-relaxed"
                style={
                  e.role === "user"
                    ? { background: `color-mix(in srgb, ${THREAD_COLOR[thread]} 16%, var(--surface-2))`, border: `1px solid color-mix(in srgb, ${THREAD_COLOR[thread]} 28%, transparent)`, color: "var(--text)" }
                    : { background: "var(--surface-2)", border: "1px solid var(--line)", color: "var(--text)" }
                }
              >
                {e.text && <div className="whitespace-pre-wrap break-words select-text">{e.text}</div>}
                <Attachments items={e.attachments} />
                <div className="text-[8px] text-ink-muted mt-1 text-right">{timeOf(e.ts)}</div>
              </div>
            </div>
          )
        )}
        {working && (
          <div className="flex items-center gap-1.5 text-[10px] text-ink-muted px-1">
            <Loader2 size={10} className="animate-spin" /> {status === "attention" ? "waiting for input in its terminal" : "working…"}
          </div>
        )}
      </div>

      {/* Composer */}
      <div className="border-t border-line p-2 shrink-0">
        {files.length > 0 && (
          <div className="flex flex-wrap gap-1 mb-1.5">
            {files.map((f) => (
              <span key={f} className="flex items-center gap-1 text-[9px] text-ink-2 bg-surface-2 rounded px-1.5 py-0.5">
                {f.split("/").pop()}
                <button onClick={() => setFiles(files.filter((x) => x !== f))} className="text-ink-muted hover:text-danger-text">
                  <X size={9} />
                </button>
              </span>
            ))}
          </div>
        )}
        <div className="flex items-end gap-1.5">
          <button
            onClick={() => pickFile().then((p) => p && setFiles((fs) => [...fs, p])).catch(console.error)}
            title="Attach a file"
            className="p-1.5 text-ink-muted hover:text-ink shrink-0"
          >
            <Paperclip size={13} />
          </button>
          <textarea
            ref={inputRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); }
              if (e.key === "Escape") close();
            }}
            rows={Math.min(6, Math.max(1, text.split("\n").length))}
            placeholder={`Message ${label} (${thread})…`}
            className="flex-1 resize-none bg-term border border-line rounded-lg px-2.5 py-1.5 text-[11px] text-white placeholder-ink-faint focus:outline-none"
            style={{ borderColor: text ? `color-mix(in srgb, ${THREAD_COLOR[thread]} 50%, transparent)` : undefined }}
          />
          <button
            onClick={send}
            disabled={sending || (!text.trim() && !files.length)}
            title="Send (Enter) — Shift+Enter for a new line"
            className="p-1.5 rounded-lg shrink-0 disabled:opacity-30"
            style={{ color: THREAD_COLOR[thread] }}
          >
            <Send size={13} />
          </button>
        </div>
      </div>
    </div>
  );
}
