export type AppState = "idle" | "listening" | "reading" | "thinking" | "result" | "error";
export type Language = "da" | "en" | "auto";
export type DefaultMode = "voice" | "linkedin" | "screen";

export type SettingsState = {
  apiKey: string;
  hotkey: string;
  language: Language;
  defaultMode: DefaultMode;
  autoInsert: boolean;
  alwaysShowResult: boolean;
  enableScreenReading: boolean;
  confirmBeforeScreenshot: boolean;
  saveHistory: boolean;
  mikkelTone: boolean;
  customTone: string;
};
