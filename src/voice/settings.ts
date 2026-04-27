import type { DefaultMode, Language, SettingsState } from "./types";
export type { DefaultMode, Language, SettingsState } from "./types";

export const DEFAULT_SETTINGS: SettingsState = {
  apiKey: "",
  hotkey: "Hold Option (⌥)",
  language: "da",
  defaultMode: "voice",
  autoInsert: true,
  alwaysShowResult: true,
  enableScreenReading: true,
  confirmBeforeScreenshot: false,
  saveHistory: true,
  mikkelTone: true,
  customTone:
    "Dansk, naturlig, venlig, kort, professionel, ikke for sælgende, ikke AI-agtig, ingen emojis som standard.",
};

export function loadSettings(): SettingsState {
  const saved = localStorage.getItem("hey-mikkel-settings");
  if (!saved) return DEFAULT_SETTINGS;

  try {
    const parsed = { ...DEFAULT_SETTINGS, ...JSON.parse(saved) } as SettingsState;
    const usesOldHotkey =
      parsed.hotkey === "CommandOrControl+Shift+Space" || parsed.hotkey === "CommandOrControl+Shift+M";
    const wasCmdHold = parsed.hotkey === "Hold CMD";
    const isBrokenHotkeyValue =
      !parsed.hotkey ||
      parsed.hotkey === "$" ||
      (parsed.hotkey.length < 6 &&
        !parsed.hotkey.toLowerCase().startsWith("hold") &&
        !parsed.hotkey.toLowerCase().includes("command") &&
        !parsed.hotkey.toLowerCase().includes("option"));
    return {
      ...parsed,
      autoInsert: usesOldHotkey ? true : parsed.autoInsert,
      hotkey:
        usesOldHotkey || isBrokenHotkeyValue || wasCmdHold ? DEFAULT_SETTINGS.hotkey : parsed.hotkey,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

/** Læses ved hvert PTT, så overlay altid har nyeste key efter indstillinger ændret i main. */
export function getSettings(): SettingsState {
  return loadSettings();
}
