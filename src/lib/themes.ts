export type ThemeMode = "light" | "dark" | "system";

export type ColorPalette =
  | "github"
  | "github-dimmed"
  | "github-hc"
  | "catppuccin"
  | "one-dark"
  | "nord";

export interface PaletteOption {
  id: ColorPalette;
  label: string;
  description: string;
  preview: { bg: string; surface: string; accent: string };
  primer?: { light: string; dark: string };
}

export const PALETTE_OPTIONS: PaletteOption[] = [
  {
    id: "github",
    label: "GitHub",
    description: "Default GitHub.com light and dark",
    preview: { bg: "#ffffff", surface: "#f6f8fa", accent: "#0969da" },
    primer: { light: "light", dark: "dark" },
  },
  {
    id: "github-dimmed",
    label: "GitHub Dimmed",
    description: "Softer dark theme from GitHub settings",
    preview: { bg: "#212830", surface: "#262c36", accent: "#478be6" },
    primer: { light: "light", dark: "dark_dimmed" },
  },
  {
    id: "github-hc",
    label: "GitHub High Contrast",
    description: "Higher contrast for accessibility",
    preview: { bg: "#ffffff", surface: "#f0f3f6", accent: "#0349b4" },
    primer: { light: "light_high_contrast", dark: "dark_high_contrast" },
  },
  {
    id: "catppuccin",
    label: "Catppuccin",
    description: "Latte light and Mocha dark",
    preview: { bg: "#1e1e2e", surface: "#313244", accent: "#89b4fa" },
  },
  {
    id: "one-dark",
    label: "One Dark",
    description: "Atom and VS Code classic",
    preview: { bg: "#282c34", surface: "#21252b", accent: "#61afef" },
  },
  {
    id: "nord",
    label: "Nord",
    description: "Cool arctic developer palette",
    preview: { bg: "#2e3440", surface: "#3b4252", accent: "#88c0d0" },
  },
];

export function isGithubPalette(palette: ColorPalette): boolean {
  return palette.startsWith("github");
}

export function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return mode;
}

export function applyDocumentTheme(
  mode: ThemeMode,
  palette: ColorPalette,
  resolved: "light" | "dark",
) {
  const root = document.documentElement;
  const option = PALETTE_OPTIONS.find((p) => p.id === palette);

  root.setAttribute("data-palette", palette);
  root.setAttribute("data-theme", resolved);

  if (option?.primer) {
    root.setAttribute(
      "data-color-mode",
      mode === "system" ? "auto" : mode,
    );
    root.setAttribute("data-light-theme", option.primer.light);
    root.setAttribute("data-dark-theme", option.primer.dark);
  } else {
    root.removeAttribute("data-color-mode");
    root.removeAttribute("data-light-theme");
    root.removeAttribute("data-dark-theme");
  }
}
