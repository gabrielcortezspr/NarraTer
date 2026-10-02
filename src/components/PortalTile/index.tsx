import { memo, useState, useCallback, useRef, useEffect } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { X, Globe, RotateCw, ExternalLink, ArrowLeft, ArrowRight, Loader2 } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { PortalNodeData } from "@/stores/canvas";
import { openUrl, portalAttach, portalDetach, portalHistory, portalInput, portalNavigate } from "@/lib/tauri";
import { onPortalFrame, onPortalState } from "@/lib/portalBus";
import type { Node, NodeProps } from "@xyflow/react";
import DevicePortalTile from "./DevicePortalTile";

type PortalNode = Node<PortalNodeData, "portal">;

const ACCENT = "#22d3ee";
const MOVE_THROTTLE_MS = 40;

// A real browser tab (headless Chrome driven over CDP by the backend): the
// tile paints its screencast and forwards mouse/keyboard, so it scales and
// moves with the canvas like any node — and agents connected to it automate
// the very same tab with `narrater portal …`.
type Status = "connecting" | "live" | "error";

const CDP_BUTTON: Record<number, string> = { 0: "left", 1: "middle", 2: "right" };

function modifiersOf(e: { altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }): number {
  return (e.altKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.metaKey ? 4 : 0) | (e.shiftKey ? 8 : 0);
}

/** Editing commands for ctrl/cmd shortcuts — headless Chrome won't run them by itself. */
const SHORTCUT_COMMANDS: Record<string, string> = { a: "selectAll", c: "copy", x: "cut", z: "undo" };

function BrowserPortalTile({ id, data, selected }: NodeProps<PortalNode>) {
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const removeNode = useCanvasStore((s) => s.removeNode);

  const [urlInput, setUrlInput] = useState(data.url ?? "");
  const [status, setStatus] = useState<Status>("connecting");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const frameSize = useRef({ width: 1280, height: 800 });
  const lastMove = useRef(0);

  // Address bar follows navigations (clicks, agents) unless the user is typing
  useEffect(() => {
    if (document.activeElement !== inputRef.current) setUrlInput(data.url ?? "");
  }, [data.url]);

  // Stream the tab while the tile is mounted (culling/LOD unmounts it)
  useEffect(() => {
    if (!data.url) inputRef.current?.focus();
    const offFrame = onPortalFrame(id, (f) => {
      if (imgRef.current) imgRef.current.src = `data:image/jpeg;base64,${f.data}`;
      if (f.width && f.height) frameSize.current = { width: f.width, height: f.height };
      setStatus("live");
    });
    const offState = onPortalState(id, (s) => {
      if (s.loading !== undefined) setLoading(s.loading);
      if (s.error) { setStatus("error"); setError(s.error); }
    });
    portalAttach(id, data.url || undefined).catch((e) => { setStatus("error"); setError(String(e)); });
    return () => {
      offFrame();
      offState();
      portalDetach(id).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const navigate = useCallback(() => {
    const url = urlInput.trim();
    if (!url) return;
    setStatus((s) => (s === "error" ? "connecting" : s));
    portalNavigate(id, url).catch((e) => setError(String(e)));
    surfaceRef.current?.focus();
  }, [id, urlInput]);

  /** Tile coordinates → page CSS px (the frame is letterboxed with object-contain). */
  const toPage = useCallback((clientX: number, clientY: number) => {
    const rect = surfaceRef.current!.getBoundingClientRect();
    const { width, height } = frameSize.current;
    const scale = Math.min(rect.width / width, rect.height / height);
    const ox = (rect.width - width * scale) / 2;
    const oy = (rect.height - height * scale) / 2;
    return {
      x: Math.max(0, Math.min(width, (clientX - rect.left - ox) / scale)),
      y: Math.max(0, Math.min(height, (clientY - rect.top - oy) / scale)),
    };
  }, []);

  const sendMouse = useCallback(
    (type: string, e: React.MouseEvent, extra: Record<string, unknown> = {}) => {
      const { x, y } = toPage(e.clientX, e.clientY);
      portalInput(id, "Input.dispatchMouseEvent", {
        type, x, y, modifiers: modifiersOf(e), buttons: e.buttons,
        button: type === "mouseMoved" ? (e.buttons & 1 ? "left" : "none") : CDP_BUTTON[e.button] ?? "left",
        ...extra,
      }).catch(() => {});
    },
    [id, toPage]
  );

  const onKey = useCallback(
    (e: React.KeyboardEvent, down: boolean) => {
      const shortcut = e.ctrlKey || e.metaKey;
      // Paste goes through the paste event (the page can't read our clipboard)
      if (shortcut && e.key.toLowerCase() === "v") return;
      e.preventDefault();
      e.stopPropagation(); // canvas shortcuts (Delete, tools) must not fire
      const printable = e.key.length === 1 && !shortcut;
      const text = e.key === "Enter" ? "\r" : printable ? e.key : undefined;
      const params: Record<string, unknown> = {
        type: down ? (text ? "keyDown" : "rawKeyDown") : "keyUp",
        key: e.key,
        code: e.code,
        windowsVirtualKeyCode: e.keyCode,
        nativeVirtualKeyCode: e.keyCode,
        modifiers: modifiersOf(e),
      };
      if (down && text) Object.assign(params, { text, unmodifiedText: text });
      const command = shortcut && down ? SHORTCUT_COMMANDS[e.key.toLowerCase()] : undefined;
      if (command) params.commands = [e.shiftKey && command === "undo" ? "redo" : command];
      portalInput(id, "Input.dispatchKeyEvent", params).catch(() => {});
    },
    [id]
  );

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden"
      style={{
        background: "#0c1a1d",
        boxShadow: selected
          ? `0 0 0 1px ${ACCENT}, 0 8px 32px rgba(0,0,0,0.5)`
          : "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(34,211,238,0.15)",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={320}
        minHeight={240}
        isVisible={selected}
        lineStyle={{ borderColor: ACCENT }}
        handleStyle={{ borderColor: ACCENT, background: "#0c1a1d" }}
      />

      {/* Name + address bar (also the drag handle) */}
      <div
        className="flex items-center gap-1.5 px-2.5 py-2 shrink-0 cursor-grab select-none"
        style={{ background: "#0f2226", borderBottom: "1px solid #163238" }}
        title={data.title}
      >
        {loading ? (
          <Loader2 size={12} style={{ color: ACCENT }} className="shrink-0 animate-spin" />
        ) : (
          <Globe size={12} style={{ color: ACCENT }} className="shrink-0" />
        )}
        <span className="text-[10px] font-medium shrink-0 max-w-[90px] truncate" style={{ color: ACCENT }}>
          {data.label}
        </span>
        {[
          { icon: <ArrowLeft size={11} />, title: "Back", run: () => portalHistory(id, -1) },
          { icon: <ArrowRight size={11} />, title: "Forward", run: () => portalHistory(id, 1) },
          { icon: <RotateCw size={11} />, title: "Reload", run: () => portalHistory(id, 0) },
        ].map((b) => (
          <button
            key={b.title}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => b.run().catch(console.error)}
            className="text-[#557] hover:text-white transition-colors p-0.5 nodrag shrink-0"
            title={b.title}
          >
            {b.icon}
          </button>
        ))}
        <input
          ref={inputRef}
          value={urlInput}
          onChange={(e) => setUrlInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && navigate()}
          onMouseDown={(e) => e.stopPropagation()}
          placeholder="example.com"
          spellCheck={false}
          className="flex-1 min-w-0 bg-[#0c1a1d] border border-[#163238] rounded px-2 py-0.5
            text-[11px] text-[#9adbe5] placeholder-[#2a4a52] outline-none focus:border-[#22d3ee60] nodrag"
        />
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => data.url && openUrl(data.url).catch(console.error)}
          className="text-[#557] hover:text-white transition-colors p-0.5 nodrag shrink-0"
          title="Open in external browser"
        >
          <ExternalLink size={11} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="text-[#666] hover:text-[#f87171] transition-colors p-0.5 rounded nodrag shrink-0"
          title="Close portal"
        >
          <X size={12} />
        </button>
      </div>

      {/* Page: screencast of the tab, input forwarded to it */}
      <div
        ref={surfaceRef}
        tabIndex={0}
        className="portal-surface flex-1 min-h-0 relative nodrag nowheel nopan nokey outline-none bg-[#0c1a1d]"
        onMouseDown={(e) => {
          e.stopPropagation();
          surfaceRef.current?.focus();
          sendMouse("mousePressed", e, { clickCount: e.detail || 1 });
        }}
        onMouseUp={(e) => sendMouse("mouseReleased", e, { clickCount: e.detail || 1 })}
        onMouseMove={(e) => {
          const now = performance.now();
          if (now - lastMove.current < MOVE_THROTTLE_MS) return;
          lastMove.current = now;
          sendMouse("mouseMoved", e);
        }}
        onWheel={(e) => {
          const { x, y } = toPage(e.clientX, e.clientY);
          portalInput(id, "Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: e.deltaX, deltaY: e.deltaY }).catch(() => {});
        }}
        onKeyDown={(e) => onKey(e, true)}
        onKeyUp={(e) => onKey(e, false)}
        onPaste={(e) => {
          const text = e.clipboardData.getData("text");
          if (text) portalInput(id, "Input.insertText", { text }).catch(() => {});
        }}
        onContextMenu={(e) => e.preventDefault()}
      >
        <img ref={imgRef} alt="" draggable={false} className="w-full h-full object-contain select-none pointer-events-none" />
        {status !== "live" && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-6 text-center">
            {status === "connecting" ? (
              <span className="flex items-center gap-2 text-[11px] text-[#2a4a52]">
                <Loader2 size={12} className="animate-spin" /> {data.url ? "Starting the browser…" : "Type a URL above"}
              </span>
            ) : (
              <>
                <span className="text-[11px] text-[#f87171]">{error}</span>
                {data.url && (
                  <button
                    onClick={() => openUrl(data.url).catch(console.error)}
                    className="text-[10px] text-[#557] hover:text-white underline"
                  >
                    Open in the external browser
                  </button>
                )}
              </>
            )}
          </div>
        )}
      </div>

      <Handle type="target" position={Position.Left} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

/** A portal shows a browser tab, or an Android device when it names one. */
function PortalTile(props: NodeProps<PortalNode>) {
  return props.data.device ? <DevicePortalTile {...props} /> : <BrowserPortalTile {...props} />;
}

export default memo(PortalTile);
