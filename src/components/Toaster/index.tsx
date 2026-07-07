import { AnimatePresence, motion } from "framer-motion";
import { AlertTriangle, CheckCircle2, X, XCircle } from "lucide-react";
import { useToastsStore } from "@/stores/toasts";
import type { ToastKind } from "@/stores/toasts";

const KIND_STYLE: Record<ToastKind, { color: string; icon: React.ReactNode }> = {
  error: { color: "var(--status-exited)", icon: <XCircle size={13} /> },
  warning: { color: "var(--status-spawning)", icon: <AlertTriangle size={13} /> },
  success: { color: "var(--status-running)", icon: <CheckCircle2 size={13} /> },
};

/** Discreet toasts in the bottom-right corner, in the app's dark theme. */
export default function Toaster() {
  const toasts = useToastsStore((s) => s.toasts);
  const dismiss = useToastsStore((s) => s.dismiss);

  return (
    <div className="fixed bottom-4 right-4 z-[60] flex flex-col gap-2 items-end pointer-events-none">
      <AnimatePresence>
        {toasts.map((t) => {
          const style = KIND_STYLE[t.kind];
          return (
            <motion.div
              key={t.id}
              initial={{ opacity: 0, y: 8, scale: 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 4, scale: 0.97 }}
              transition={{ duration: 0.15 }}
              className="pointer-events-auto flex items-start gap-2 max-w-[340px] bg-canvas-tile border border-canvas-border rounded-lg shadow-xl px-3 py-2"
            >
              <span className="shrink-0 mt-px" style={{ color: style.color }}>
                {style.icon}
              </span>
              <span className="text-[11px] text-ink leading-snug break-words min-w-0">{t.text}</span>
              <button
                onClick={() => dismiss(t.id)}
                aria-label="Dismiss notification"
                className="text-ink-faint hover:text-ink transition-colors shrink-0 mt-px"
              >
                <X size={11} />
              </button>
            </motion.div>
          );
        })}
      </AnimatePresence>
    </div>
  );
}
