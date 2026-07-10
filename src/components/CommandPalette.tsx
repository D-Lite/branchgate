import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { modLabel } from "../lib/keyboard";
import "./CommandPalette.css";

export interface PalettePipeline {
  id: number;
  name: string;
  pendingCount: number;
}

interface CommandPaletteProps {
  open: boolean;
  pipelines: PalettePipeline[];
  onClose: () => void;
}

export function CommandPalette({ open, pipelines, onClose }: CommandPaletteProps) {
  const navigate = useNavigate();
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return pipelines;
    return pipelines.filter((p) => p.name.toLowerCase().includes(q));
  }, [pipelines, query]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    const t = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, [open]);

  useEffect(() => {
    setActiveIndex((i) => Math.min(i, Math.max(0, filtered.length - 1)));
  }, [filtered.length]);

  useEffect(() => {
    if (!open) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, filtered.length - 1));
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
        return;
      }
      if (event.key === "Enter" && filtered[activeIndex]) {
        event.preventDefault();
        navigate(`/pipeline/${filtered[activeIndex].id}`);
        onClose();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, filtered, activeIndex, navigate, onClose]);

  if (!open) return null;

  const go = (id: number) => {
    navigate(`/pipeline/${id}`);
    onClose();
  };

  return (
    <div className="command-palette-backdrop" onClick={onClose}>
      <div
        className="command-palette card"
        role="dialog"
        aria-modal="true"
        aria-label="Go to pipeline"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          className="command-palette-input mono"
          placeholder="Go to pipeline…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <ul className="command-palette-list">
          {filtered.length === 0 ? (
            <li className="command-palette-empty">No matching pipelines</li>
          ) : (
            filtered.map((pipeline, index) => (
              <li key={pipeline.id}>
                <button
                  type="button"
                  className={`command-palette-item${
                    index === activeIndex ? " command-palette-item-active" : ""
                  }`}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => go(pipeline.id)}
                >
                  <span className="command-palette-name">{pipeline.name}</span>
                  {pipeline.pendingCount > 0 && (
                    <span className="command-palette-meta mono">
                      {pipeline.pendingCount} pending
                    </span>
                  )}
                </button>
              </li>
            ))
          )}
        </ul>
        <div className="command-palette-footer mono">
          <span>↑↓ navigate</span>
          <span>↵ open</span>
          <span>esc close</span>
          <span>{modLabel()}P</span>
        </div>
      </div>
    </div>
  );
}
