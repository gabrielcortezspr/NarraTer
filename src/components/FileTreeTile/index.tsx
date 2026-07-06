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

const ACCENT = "#60a5fa";

function FileTreeTile({ id, data, selected }: NodeProps<FileTreeNode>) {
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const removeNode = useCanvasStore((s) => s.removeNode);

  const [rootInput, setRootInput] = useState(data.rootPath);
  // Cache local (não persistido) do conteúdo de cada diretório carregado;
  // a expansão em si vive em data.expandedPaths e sobrevive ao save.
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

  // Carga inicial: raiz + diretórios que estavam expandidos no save
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
              className="group flex items-center gap-1 py-0.5 pr-1 rounded hover:bg-[#1a2230] cursor-pointer relative"
              style={{ paddingLeft: depth * 12 + 6 }}
              onClick={() => e.is_dir && toggleDir(e.path)}
            >
              {e.is_dir ? (
                <>
                  {expanded ? <ChevronDown size={10} className="text-[#4a5568] shrink-0" /> : <ChevronRight size={10} className="text-[#4a5568] shrink-0" />}
                  {expanded ? <FolderOpen size={11} className="shrink-0" style={{ color: ACCENT }} /> : <Folder size={11} className="shrink-0" style={{ color: ACCENT }} />}
                </>
              ) : (
                <FileIcon size={11} className="text-[#556] shrink-0 ml-[10px]" />
              )}
              <span className="text-[11px] text-[#aab] truncate flex-1">{e.name}</span>

              {!e.is_dir && (
                <button
                  onClick={(ev) => {
                    ev.stopPropagation();
                    setEditorMenuFor(editorMenuFor === e.path ? null : e.path);
                  }}
                  className="opacity-0 group-hover:opacity-100 text-[#556] hover:text-white transition-opacity shrink-0 p-0.5"
                  title="Abrir no editor"
                >
                  <ExternalLink size={10} />
                </button>
              )}

              {editorMenuFor === e.path && (
                <div
                  className="absolute right-0 top-5 z-20 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg shadow-xl py-1 min-w-[100px]"
                  onClick={(ev) => ev.stopPropagation()}
                >
                  {EDITORS.map((ed) => (
                    <button
                      key={ed.cmd}
                      onClick={() => {
                        openInEditor(ed.cmd, e.path).catch(console.error);
                        setEditorMenuFor(null);
                      }}
                      className="block w-full text-left px-3 py-1 text-[11px] text-[#ccc] hover:bg-[#2a2a2a] hover:text-white"
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
      className="flex flex-col w-full h-full rounded-xl overflow-hidden"
      style={{
        background: "#0e141d",
        boxShadow: selected
          ? `0 0 0 1px ${ACCENT}, 0 8px 32px rgba(0,0,0,0.5)`
          : "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(96,165,250,0.15)",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={200}
        minHeight={180}
        isVisible={selected}
        lineStyle={{ borderColor: ACCENT }}
        handleStyle={{ borderColor: ACCENT, background: "#0e141d" }}
      />

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab select-none"
        style={{ background: "#111927", borderBottom: "1px solid #1a2436" }}
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
          title="Enter recarrega a raiz"
        />
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => setShowHidden((v) => !v)}
          className="text-[#556] hover:text-white transition-colors p-0.5 nodrag"
          title={showHidden ? "Ocultar dotfiles" : "Mostrar dotfiles"}
        >
          {showHidden ? <Eye size={11} /> : <EyeOff size={11} />}
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={refresh}
          className="text-[#556] hover:text-white transition-colors p-0.5 nodrag"
          title="Recarregar"
        >
          <RefreshCw size={11} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="text-[#666] hover:text-[#f87171] transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Árvore */}
      <div
        className="flex-1 overflow-y-auto py-1 px-1 nodrag nowheel"
        onMouseDown={(e) => e.stopPropagation()}
      >
        {error ? (
          <div className="text-[10px] text-[#f87171] px-2 py-1 break-all">{error}</div>
        ) : (
          renderDir(data.rootPath, 0)
        )}
      </div>

      <Handle type="target" position={Position.Left} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

export default memo(FileTreeTile);
