import { memo, useState, useCallback, useEffect } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import {
  X, Folder, FolderOpen, File as FileIcon, RefreshCw,
  Eye, EyeOff, ChevronRight, ChevronDown, ExternalLink,
} from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { FileTreeNodeData } from "@/stores/canvas";
import { fsListDir, openInEditor, type FsEntry } from "@/lib/tauri";
import { EDITORS } from "@/lib/editors";
import type { Node, NodeProps } from "@xyflow/react";

type FileTreeNode = Node<FileTreeNodeData, "filetree">;

// Identity of this node kind: icon and top thread only (docs/design.md)
const ACCENT = "#7aa7ff";

function FileTreeTile({ id, data, selected }: NodeProps<FileTreeNode>) {
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const removeNode = useCanvasStore((s) => s.removeNode);

  const [rootInput, setRootInput] = useState(data.rootPath);
  // Local (non-persisted) cache of each loaded directory's contents;
  // the expansion itself lives in data.expandedPaths and survives saves.
  const [entriesByDir, setEntriesByDir] = useState<Map<string, FsEntry[]>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [editorMenuFor, setEditorMenuFor] = useState<string | null>(null);

  const loadDir = useCallback(async (path: string) => {
    try {
      const entries = await fsListDir(path);
      setEntriesByDir((prev) => new Map(prev).set(path, entries));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Initial load: root + directories that were expanded at save time
  useEffect(() => {
    loadDir(data.rootPath);
    data.expandedPaths.forEach((p) => loadDir(p));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggleDir = useCallback(
    (path: string) => {
      if (data.expandedPaths.includes(path)) {
        updateNodeData(id, { expandedPaths: data.expandedPaths.filter((p) => p !== path) });
      } else {
        updateNodeData(id, { expandedPaths: [...data.expandedPaths, path] });
        loadDir(path);
      }
    },
    [id, data.expandedPaths, updateNodeData, loadDir]
  );

  const applyRoot = useCallback(() => {
    const root = rootInput.trim() || "~";
    updateNodeData(id, { rootPath: root, expandedPaths: [] });
    setEntriesByDir(new Map());
    loadDir(root);
  }, [id, rootInput, updateNodeData, loadDir]);

  const refresh = useCallback(() => {
    loadDir(data.rootPath);
    data.expandedPaths.forEach((p) => loadDir(p));
  }, [data.rootPath, data.expandedPaths, loadDir]);

  const renderDir = (path: string, depth: number): React.ReactNode => {
    const entries = entriesByDir.get(path);
    if (!entries) return null;
    return entries
      .filter((e) => showHidden || !e.name.startsWith("."))
      .map((e) => {
        const expanded = data.expandedPaths.includes(e.path);
        return (
          <div key={e.path}>
            <div
              className="group flex items-center gap-1 py-0.5 pr-1 rounded hover:bg-info/10 cursor-pointer relative"
              style={{ paddingLeft: depth * 12 + 6 }}
              onClick={() => e.is_dir && toggleDir(e.path)}
            >
              {e.is_dir ? (
                <>
                  {expanded ? <ChevronDown size={10} className="text-ink-muted shrink-0" /> : <ChevronRight size={10} className="text-ink-muted shrink-0" />}
                  {expanded ? <FolderOpen size={11} className="shrink-0" style={{ color: ACCENT }} /> : <Folder size={11} className="shrink-0" style={{ color: ACCENT }} />}
                </>
              ) : (
                <FileIcon size={11} className="text-ink-muted shrink-0 ml-[10px]" />
              )}
              <span className="text-[11px] text-ink-2 truncate flex-1">{e.name}</span>

              {!e.is_dir && (
                <button
                  onClick={(ev) => {
                    ev.stopPropagation();
                    setEditorMenuFor(editorMenuFor === e.path ? null : e.path);
                  }}
                  className="opacity-0 group-hover:opacity-100 text-ink-muted hover:text-ink transition-opacity shrink-0 p-0.5"
                  title="Open in editor"
                >
                  <ExternalLink size={10} />
                </button>
              )}

              {editorMenuFor === e.path && (
                <div
                  className="absolute right-0 top-5 z-20 bg-raised border border-line rounded-lg shadow-xl py-1 min-w-[100px]"
                  onClick={(ev) => ev.stopPropagation()}
                >
                  {EDITORS.map((ed) => (
                    <button
                      key={ed.cmd}
                      onClick={() => {
                        openInEditor(ed.cmd, e.path).catch(console.error);
                        setEditorMenuFor(null);
                      }}
                      className="block w-full text-left px-3 py-1 text-[11px] text-ink hover:bg-hover hover:text-ink"
                    >
                      {ed.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
            {e.is_dir && expanded && renderDir(e.path, depth + 1)}
          </div>
        );
      });
  };

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden bg-surface border border-line transition-shadow duration-150"
      style={{ boxShadow: selected ? "var(--ring-accent), var(--shadow-tile)" : "var(--shadow-tile)" }}
    >
      <div className="h-px shrink-0 opacity-80" style={{ background: `linear-gradient(90deg, transparent 4%, ${ACCENT} 50%, transparent 96%)` }} />
      <NodeResizer
        minWidth={200}
        minHeight={180}
        isVisible={selected}
        lineStyle={{ borderColor: "transparent" }}
        handleStyle={{ width: 8, height: 8, borderRadius: 2, border: "1.5px solid var(--accent)", background: "var(--surface)" }}
      />

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab select-none"
        
      >
        <Folder size={12} style={{ color: ACCENT }} className="shrink-0" />
        <input
          value={rootInput}
          onChange={(e) => setRootInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && applyRoot()}
          onMouseDown={(e) => e.stopPropagation()}
          spellCheck={false}
          className="flex-1 min-w-0 bg-transparent text-[11px] outline-none nodrag"
          style={{ color: ACCENT }}
          title="Enter reloads the root"
        />
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => setShowHidden((v) => !v)}
          className="text-ink-muted hover:text-ink transition-colors p-0.5 nodrag"
          title={showHidden ? "Hide dotfiles" : "Show dotfiles"}
        >
          {showHidden ? <Eye size={11} /> : <EyeOff size={11} />}
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={refresh}
          className="text-ink-muted hover:text-ink transition-colors p-0.5 nodrag"
          title="Reload"
        >
          <RefreshCw size={11} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="text-ink-muted hover:text-danger-text transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Tree */}
      <div
        className="flex-1 overflow-y-auto py-1 px-1 nodrag nowheel"
        onMouseDown={(e) => e.stopPropagation()}
      >
        {error ? (
          <div className="text-[10px] text-danger-text px-2 py-1 break-all">{error}</div>
        ) : (
          renderDir(data.rootPath, 0)
        )}
      </div>

      <Handle type="target" position={Position.Left} className="narrater-handle" />
      <Handle type="source" position={Position.Right} className="narrater-handle" />
    </div>
  );
}

export default memo(FileTreeTile);
