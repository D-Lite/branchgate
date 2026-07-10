import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  applyDocumentTheme,
  resolveTheme,
  type ColorPalette,
  type ThemeMode,
} from "../lib/themes";

interface ThemeContextValue {
  theme: "light" | "dark";
  mode: ThemeMode;
  palette: ColorPalette;
  setMode: (mode: ThemeMode) => void;
  setPalette: (palette: ColorPalette) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const MODE_KEY = "branchgate-theme";
const PALETTE_KEY = "branchgate-palette";

function readMode(): ThemeMode {
  const stored = localStorage.getItem(MODE_KEY) as ThemeMode | null;
  if (stored === "light" || stored === "dark" || stored === "system") {
    return stored;
  }
  return "system";
}

function readPalette(): ColorPalette {
  const stored = localStorage.getItem(PALETTE_KEY) as ColorPalette | null;
  if (
    stored === "github" ||
    stored === "github-dimmed" ||
    stored === "github-hc" ||
    stored === "catppuccin" ||
    stored === "one-dark" ||
    stored === "nord"
  ) {
    return stored;
  }
  return "github";
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [mode, setModeState] = useState<ThemeMode>(readMode);
  const [palette, setPaletteState] = useState<ColorPalette>(readPalette);
  const [theme, setTheme] = useState<"light" | "dark">(() =>
    resolveTheme(readMode()),
  );

  const syncDocument = useCallback(
    (nextMode: ThemeMode, nextPalette: ColorPalette) => {
      const resolved = resolveTheme(nextMode);
      setTheme(resolved);
      applyDocumentTheme(nextMode, nextPalette, resolved);
    },
    [],
  );

  const setMode = useCallback(
    (next: ThemeMode) => {
      setModeState(next);
      localStorage.setItem(MODE_KEY, next);
      syncDocument(next, palette);
    },
    [palette, syncDocument],
  );

  const setPalette = useCallback(
    (next: ColorPalette) => {
      setPaletteState(next);
      localStorage.setItem(PALETTE_KEY, next);
      syncDocument(mode, next);
    },
    [mode, syncDocument],
  );

  useEffect(() => {
    syncDocument(mode, palette);
  }, [mode, palette, syncDocument]);

  useEffect(() => {
    if (mode !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => syncDocument("system", palette);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [mode, palette, syncDocument]);

  const value = useMemo(
    () => ({ theme, mode, palette, setMode, setPalette }),
    [theme, mode, palette, setMode, setPalette],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
