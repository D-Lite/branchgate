import { formatShortcut } from "../lib/keyboard";
import "./ShortcutsHelp.css";

interface ShortcutsHelpProps {
  open: boolean;
  onClose: () => void;
}

const SECTIONS = [
  {
    title: "Navigation",
    shortcuts: [
      { keys: ["Mod", "P"], label: "Go to pipeline…" },
      { keys: ["Mod", "1"], label: "Pipelines dashboard" },
      { keys: ["Mod", "2"], label: "History" },
      { keys: ["Mod", ","], label: "Settings" },
      { keys: ["Mod", "["], label: "Previous pipeline" },
      { keys: ["Mod", "]"], label: "Next pipeline" },
    ],
  },
  {
    title: "Actions",
    shortcuts: [
      { keys: ["Mod", "N"], label: "New pipeline / connect repo" },
      { keys: ["Mod", "R"], label: "Refresh current pipeline" },
      { keys: ["Mod", "Enter"], label: "Promote selected" },
      { keys: ["Mod", "A"], label: "Select all pending" },
    ],
  },
  {
    title: "General",
    shortcuts: [
      { keys: ["?"], label: "Keyboard shortcuts" },
      { keys: ["Esc"], label: "Close palette, modal, or help" },
    ],
  },
];

export function ShortcutsHelp({ open, onClose }: ShortcutsHelpProps) {
  if (!open) return null;

  return (
    <div className="shortcuts-help-backdrop" onClick={onClose}>
      <div
        className="shortcuts-help card"
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="shortcuts-help-head">
          <h2>Keyboard shortcuts</h2>
          <button type="button" className="btn btn-subtle" onClick={onClose}>
            Close
          </button>
        </div>

        {SECTIONS.map((section) => (
          <section key={section.title} className="shortcuts-help-section">
            <h3>{section.title}</h3>
            <dl className="shortcuts-help-list">
              {section.shortcuts.map((item) => (
                <div key={item.label}>
                  <dt className="mono">{formatShortcut(item.keys)}</dt>
                  <dd>{item.label}</dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </div>
  );
}
