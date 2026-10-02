import { memo, useCallback, useEffect, useRef, useState } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { X, Smartphone, ArrowLeft, Circle, Square, Loader2 } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { PortalNodeData } from "@/stores/canvas";
import { deviceInput, portalAttach, portalDetach } from "@/lib/tauri";
import { onPortalFrame } from "@/lib/portalBus";
import type { Node, NodeProps } from "@xyflow/react";

type PortalNode = Node<PortalNodeData, "portal">;

const ACCENT = "#a3e635";
/** A press that moves less than this (device px) is a tap, not a swipe. */
const TAP_SLOP = 12;
const WHEEL_FLUSH_MS = 120;

// An Android emulator or phone (adb) on the canvas: the tile shows its
// screen and turns mouse/keyboard into taps, swipes, keys and text. Agents
// wired to it drive the same device with `narrater portal …`.
const KEYS: Record<string, string> = {
  Enter: "Enter", Backspace: "Backspace", Tab: "Tab", Delete: "Delete", Escape: "back",
  ArrowUp: "ArrowUp", ArrowDown: "ArrowDown", ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight",
};

function DevicePortalTile({ id, data, selected }: NodeProps<PortalNode>) {
  const removeNode = useCanvasStore((s) => s.removeNode);
  const [status, setStatus] = useState<"connecting" | "live" | "error">("connecting");
  const [error, setError] = useState("");
  const surfaceRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const screen = useRef({ width: 1080, height: 2400 });
  const press = useRef<{ x: number; y: number; t: number } | null>(null);
  const wheel = useRef({ dy: 0, dx: 0, x: 0, y: 0, timer: 0 });

  useEffect(() => {
    const off = onPortalFrame(id, (f) => {
      if (imgRef.current) imgRef.current.src = `data:image/jpeg;base64,${f.data}`;
      if (f.width && f.height) screen.current = { width: f.width, height: f.height };
      setStatus("live");
    });
    portalAttach(id, undefined, data.device).catch((e) => { setStatus("error"); setError(String(e)); });
    return () => {
      off();
      portalDetach(id).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const send = useCallback(
    (kind: "tap" | "swipe" | "key" | "text", params: Record<string, unknown>) =>
      deviceInput(id, kind, params).catch((e) => console.warn("device input:", e)),
    [id]
  );

  /** Tile coordinates → device pixels (the screen is letterboxed). */
  const toDevice = useCallback((clientX: number, clientY: number) => {
    const rect = surfaceRef.current!.getBoundingClientRect();
    const { width, height } = screen.current;
    const scale = Math.min(rect.width / width, rect.height / height);
    const ox = (rect.width - width * scale) / 2;
    const oy = (rect.height - height * scale) / 2;
    return {
      x: Math.round(Math.max(0, Math.min(width - 1, (clientX - rect.left - ox) / scale))),
      y: Math.round(Math.max(0, Math.min(height - 1, (clientY - rect.top - oy) / scale))),
    };
  }, []);

  const flushWheel = useCallback(() => {
    const w = wheel.current;
    w.timer = 0;
    const { height, width } = screen.current;
    // Content follows the wheel: the finger moves the other way
    const dy = Math.max(-height * 0.6, Math.min(height * 0.6, w.dy * 2));
    const dx = Math.max(-width * 0.6, Math.min(width * 0.6, w.dx * 2));
    w.dy = 0;
    w.dx = 0;
    if (Math.abs(dy) < 5 && Math.abs(dx) < 5) return;
    send("swipe", { x1: w.x + dx / 2, y1: w.y + dy / 2, x2: w.x - dx / 2, y2: w.y - dy / 2, ms: 200 });
  }, [send]);

  const headerButton = (title: string, icon: React.ReactNode, key: string) => (
    <button
      key={title}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={() => send("key", { key })}
      className="text-[#5a6b3a] hover:text-white transition-colors p-0.5 nodrag shrink-0"
      title={title}
    >
      {icon}
    </button>
  );

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden"
      style={{
        background: "#101410",
        boxShadow: selected
          ? `0 0 0 1px ${ACCENT}, 0 8px 32px rgba(0,0,0,0.5)`
          : "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(163,230,53,0.15)",
      }}
    >
      <NodeResizer minWidth={220} minHeight={320} isVisible={selected} lineStyle={{ borderColor: ACCENT }} handleStyle={{ borderColor: ACCENT, background: "#101410" }} />

      <div
        className="flex items-center gap-1.5 px-2.5 py-2 shrink-0 cursor-grab select-none"
        style={{ background: "#141a12", borderBottom: "1px solid #232d1c" }}
      >
        <Smartphone size={12} style={{ color: ACCENT }} className="shrink-0" />
        <span className="text-[10px] font-medium truncate" style={{ color: ACCENT }}>{data.label}</span>
        <span className="text-[9px] text-[#4a5a32] truncate flex-1" title={data.device}>{data.device}</span>
        {headerButton("Back", <ArrowLeft size={11} />, "back")}
        {headerButton("Home", <Circle size={10} />, "home")}
        {headerButton("Recents", <Square size={9} />, "recents")}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="text-[#666] hover:text-[#f87171] transition-colors p-0.5 rounded nodrag shrink-0"
          title="Close portal"
        >
          <X size={12} />
        </button>
      </div>

      <div
        ref={surfaceRef}
        tabIndex={0}
        className="portal-surface flex-1 min-h-0 relative nodrag nowheel nopan nokey outline-none bg-black"
        onMouseDown={(e) => {
          e.stopPropagation();
          surfaceRef.current?.focus();
          if (e.button !== 0) return;
          press.current = { ...toDevice(e.clientX, e.clientY), t: performance.now() };
        }}
        onMouseUp={(e) => {
          const start = press.current;
          press.current = null;
          if (!start || e.button !== 0) return;
          const end = toDevice(e.clientX, e.clientY);
          if (Math.hypot(end.x - start.x, end.y - start.y) < TAP_SLOP) send("tap", start);
          else {
            const ms = Math.round(Math.min(2000, Math.max(100, performance.now() - start.t)));
            send("swipe", { x1: start.x, y1: start.y, x2: end.x, y2: end.y, ms });
          }
        }}
        onWheel={(e) => {
          const w = wheel.current;
          const at = toDevice(e.clientX, e.clientY);
          w.x = at.x;
          w.y = at.y;
          w.dy += e.deltaY;
          w.dx += e.deltaX;
          if (!w.timer) w.timer = window.setTimeout(flushWheel, WHEEL_FLUSH_MS);
        }}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") return; // → paste
          e.preventDefault();
          e.stopPropagation();
          if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) send("text", { text: e.key });
          else if (KEYS[e.key]) send("key", { key: KEYS[e.key] });
        }}
        onPaste={(e) => {
          const text = e.clipboardData.getData("text");
          if (text) send("text", { text });
        }}
        onContextMenu={(e) => e.preventDefault()}
      >
        <img ref={imgRef} alt="" draggable={false} className="w-full h-full object-contain select-none pointer-events-none" />
        {status !== "live" && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-6 text-center">
            {status === "connecting" ? (
              <span className="flex items-center gap-2 text-[11px] text-[#4a5a32]">
                <Loader2 size={12} className="animate-spin" /> Connecting to {data.device}… (booting an emulator takes a minute)
              </span>
            ) : (
              <span className="text-[11px] text-[#f87171]">{error}</span>
            )}
          </div>
        )}
      </div>

      <Handle type="target" position={Position.Left} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

export default memo(DevicePortalTile);
