// The user's light/dark choice; "system" follows the computer's setting. It lives in
// localStorage so main.tsx can apply it before the first render, with no flash of the wrong theme.
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemeChoice = "system" | "light" | "dark";

const KEY = "porthop.theme";

export function loadTheme(): ThemeChoice {
  try {
    const saved = localStorage.getItem(KEY);
    return saved === "light" || saved === "dark" ? saved : "system";
  } catch {
    return "system";
  }
}

export function applyTheme(choice: ThemeChoice) {
  const root = document.documentElement;
  if (choice === "system") delete root.dataset.theme;
  else root.dataset.theme = choice;
  // The window's title bar too (on Windows this also sets what the page sees as the system theme).
  getCurrentWindow().setTheme(choice === "system" ? null : choice).catch(() => {});
}

export function saveTheme(choice: ThemeChoice) {
  try {
    localStorage.setItem(KEY, choice);
  } catch { /* still applied for this session */ }
  applyTheme(choice);
}
