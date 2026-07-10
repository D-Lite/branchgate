import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { isEditableTarget, modKeyActive } from "../lib/keyboard";
import type { ShortcutHandlers } from "./useShortcutActions";

interface PalettePipeline {
  id: number;
}

interface UseGlobalShortcutsOptions {
  pipelines: PalettePipeline[];
  paletteOpen: boolean;
  helpOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  setHelpOpen: (open: boolean) => void;
  invoke: <K extends keyof ShortcutHandlers>(key: K) => void;
}

export function useGlobalShortcuts({
  pipelines,
  paletteOpen,
  helpOpen,
  setPaletteOpen,
  setHelpOpen,
  invoke,
}: UseGlobalShortcutsOptions) {
  const navigate = useNavigate();
  const location = useLocation();

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (paletteOpen || helpOpen) {
        if (event.key === "Escape") {
          event.preventDefault();
          setPaletteOpen(false);
          setHelpOpen(false);
          invoke("closeOverlay");
        }
        return;
      }

      if (event.key === "?" && !isEditableTarget(event.target)) {
        event.preventDefault();
        setHelpOpen(true);
        return;
      }

      if (!modKeyActive(event)) return;

      const key = event.key.toLowerCase();
      const editing = isEditableTarget(event.target);

      if (key === "p") {
        event.preventDefault();
        setPaletteOpen(true);
        return;
      }

      if (key === "n") {
        event.preventDefault();
        navigate("/connect");
        return;
      }

      if (key === ",") {
        event.preventDefault();
        navigate("/settings");
        return;
      }

      if (key === "1") {
        event.preventDefault();
        navigate("/");
        return;
      }

      if (key === "2") {
        event.preventDefault();
        navigate("/history");
        return;
      }

      if (key === "r" && !event.shiftKey) {
        event.preventDefault();
        invoke("refresh");
        return;
      }

      if (key === "enter") {
        event.preventDefault();
        invoke("promote");
        return;
      }

      if (key === "a" && !editing) {
        event.preventDefault();
        invoke("selectAll");
        return;
      }

      if (key === "[" || key === "]") {
        event.preventDefault();
        const match = location.pathname.match(/^\/pipeline\/(\d+)$/);
        if (!match || pipelines.length === 0) return;

        const currentId = Number(match[1]);
        const index = pipelines.findIndex((p) => p.id === currentId);
        if (index < 0) return;

        const nextIndex =
          key === "["
            ? (index - 1 + pipelines.length) % pipelines.length
            : (index + 1) % pipelines.length;
        navigate(`/pipeline/${pipelines[nextIndex].id}`);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    paletteOpen,
    helpOpen,
    pipelines,
    location.pathname,
    navigate,
    setPaletteOpen,
    setHelpOpen,
    invoke,
  ]);
}
