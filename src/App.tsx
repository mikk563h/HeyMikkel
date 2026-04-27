import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Clipboard, Mic, Monitor, Settings, Sparkles, X } from "lucide-react";
import { useVoiceSession } from "./hooks/useVoiceSession";
import { loadSettings, type Language, type SettingsState } from "./voice/settings";
import type { DefaultMode } from "./voice/types";
import { requestNativeMicrophonePermission } from "./macosMicPermission";
import "./styles.css";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export default function App() {
  const [settings, setSettings] = useState<SettingsState>(loadSettings);
  const [showSettings, setShowSettings] = useState(!settings.apiKey);

  const {
    state,
    message,
    setMessage,
    transcript,
    result,
    setResult,
    error,
    startRecording,
    stopRecording,
    testMicrophone,
    refine,
    copyResult,
    insertResult,
    generateResponse,
    lastInstruction,
    lastScreenshot,
    history,
  } = useVoiceSession({ listenPushToTalk: false });

  useEffect(() => {
    localStorage.setItem("hey-mikkel-settings", JSON.stringify(settings));
  }, [settings]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    void invoke<boolean>("request_accessibility_permission");
  }, []);

  /** Sørg for at macOS kan vise mikrofon-prompt og tilføje appen til Mikrofon-listen (efter Regular/NSApp). */
  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    const t = window.setTimeout(() => void requestNativeMicrophonePermission(), 500);
    return () => window.clearTimeout(t);
  }, []);

  const statusText = useMemo(() => {
    if (state === "listening") return "Lytter...";
    if (state === "reading") return "Kigger på skærmen...";
    if (state === "thinking") return "Skriver svar...";
    if (state === "result") return "Svar klar";
    if (state === "error") return "Noget gik galt";
    return `Hey Mikkel · ${settings.defaultMode === "voice" ? "Voice to Text" : settings.defaultMode === "linkedin" ? "LinkedIn Reply" : "Screen Assistant"}`;
  }, [settings.defaultMode, state]);

  const updateSetting = <Key extends keyof SettingsState>(key: Key, value: SettingsState[Key]) => {
    setSettings((current) => ({ ...current, [key]: value }));
  };

  const hideToTray = () => {
    if (!window.__TAURI_INTERNALS__) return;
    void invoke("hide_main_window");
  };

  return (
    <main className="app-shell">
      <section className={`assistant-card ${state}`}>
        <div className="top-bar" data-tauri-drag-region>
          <div className="brand">
            <div className="logo">
              <Sparkles size={20} />
            </div>
            <div>
              <h1>Hey Mikkel</h1>
              <p>
                {statusText} — Hold <strong>Option (⌥)</strong> nede for at tale. Ikon i{" "}
                <strong>menu baren</strong> (øverst til højre); appen findes også i <strong>Dock</strong> så macOS kan
                give mikrofontilladelse.
              </p>
            </div>
          </div>
          <div className="top-bar-tools">
            <button
              type="button"
              className="icon-button"
              onClick={() => setShowSettings((v) => !v)}
              aria-label="Indstillinger"
            >
              <Settings size={18} />
            </button>
            {Boolean(window.__TAURI_INTERNALS__) && (
              <button type="button" className="icon-button win-minimize" onClick={hideToTray} title="Skjul (app kører videre)">
                <X size={18} />
              </button>
            )}
          </div>
        </div>

        <div className="status-panel">
          <div className="pulse">{state === "reading" ? <Monitor size={28} /> : <Mic size={28} />}</div>
          <h2>{statusText}</h2>
          <p>{message}</p>
        </div>

        <div className="push-area">
          <button
            className="talk-button"
            onMouseDown={startRecording}
            onMouseUp={stopRecording}
            onMouseLeave={stopRecording}
            onTouchStart={startRecording}
            onTouchEnd={stopRecording}
          >
            Hold for at tale (test)
          </button>
          <span>Den primære oplevelse kører i baggrunden i overlay, når du holder Option (⌥) nede.</span>
        </div>

        {transcript && (
          <div className="transcript">
            <strong>Du sagde:</strong>
            <p>{transcript}</p>
          </div>
        )}

        {state === "result" && result && (
          <div className="result-box">
            <textarea value={result} onChange={(event) => setResult(event.target.value)} />
            <div className="actions">
              <button onClick={() => copyResult()}>
                <Clipboard size={16} />
                Kopiér
              </button>
              <button onClick={() => insertResult()}>Indsæt</button>
              <button onClick={() => refine("Gør svaret kortere. Return only the final text.")}>Kortere</button>
              <button onClick={() => refine("Gør svaret mere personligt. Return only the final text.")}>
                Mere personlig
              </button>
              <button onClick={() => refine("Gør svaret mere professionelt. Return only the final text.")}>
                Mere professionel
              </button>
              <button onClick={() => generateResponse(lastInstruction, lastScreenshot)}>Prøv igen</button>
            </div>
          </div>
        )}

        {state === "error" && (
          <div className="error-box error-box--actions">
            <p>{error}</p>
            {/mikrofon|microphone|not allowed/i.test(error) && window.__TAURI_INTERNALS__ ? (
              <div className="error-actions">
                <button type="button" className="inline-button" onClick={() => void invoke("open_microphone_privacy")}>
                  Åbn Mikrofon i Systemindstillinger
                </button>
              </div>
            ) : null}
          </div>
        )}
      </section>

      {showSettings && (
        <aside className="settings-panel">
          <div>
            <h2>Indstillinger</h2>
            <p>Gemmes kun lokalt på denne computer.</p>
          </div>

          {window.__TAURI_INTERNALS__ ? (
            <div className="mic-guide">
              <h3 className="mic-guide-title">Mikrofon — så kommer appen på listen</h3>
              <p className="mic-guide-lead">
                <strong>Sikreste vej (rigtig Mac-app):</strong> i projektmappen kør{" "}
                <code>npm run tauri:macos:app</code> — så bygges <strong>Hey Mikkel.app</strong> med til bundtet
                mikrofontilladelse. Åbn den app, klik <strong>Tillad mikrofon</strong>, og tjek{" "}
                <strong>Systemindstillinger → Mikrofon</strong> — der skal <strong>Hey Mikkel</strong> stå.
                <br />
                <strong>Udvikling (tauri dev):</strong> kør <code>npm run tauri:macos:prepare</code> en gang, derefter{" "}
                <code>npm run tauri dev</code> (bruger nu lokal <code>target</code> uden Cursors ekstra sti). Tjek
                listen for <code>hey-mikkel</code> hvis navnet ikke vises som "Hey Mikkel".
              </p>
              <ol className="mic-steps">
                <li>
                  Klik <strong>Tillad mikrofon herunder</strong>.
                </li>
                <li>
                  Hvis et macOS-vindue spørger, vælg <strong>OK</strong>.
                </li>
                <li>
                  Gå til <strong>Systemindstillinger → Anonymitet og sikkerhed → Mikrofon</strong> og tjek at{" "}
                  <strong>Hey Mikkel</strong> er slået til. (Når du kører med <code>npm run tauri dev</code>, kan
                  navnet i stedet være <code>hey-mikkel</code> — det er samme app.)
                </li>
              </ol>
              <div className="mic-guide-actions">
                <button type="button" className="mic-guide-primary" onClick={() => void testMicrophone()}>
                  Tillad mikrofon
                </button>
                <button type="button" className="inline-button" onClick={() => void invoke("open_microphone_privacy")}>
                  Åbn mikrofon-panelet
                </button>
              </div>
            </div>
          ) : null}

          <label>
            OpenAI API key
            <input
              type="password"
              value={settings.apiKey}
              placeholder="sk-..."
              onChange={(event) => updateSetting("apiKey", event.target.value)}
            />
          </label>

          <div className="field-note">
            <div className="field-note-title">Push-to-talk</div>
            <p className="field-note-text">
              Hold <strong>Option (⌥ / Alt)</strong> nede (ca. ½ sekund). Under{" "}
              <strong>Fortrolighed og sikkerhed → Mikrofon</strong> skal Hey Mikkel være slået til, ellers får
              den ikke lyd. Under <strong>beskyttelse → Tilgængelighed</strong> skal appen være til, så den kan
              mærke tasterne. Brug <strong>Test mikrofon</strong> her én gang efter du har givet tilladelse.
            </p>
            <button type="button" className="inline-button" onClick={() => void testMicrophone()}>
              Test mikrofon
            </button>
          </div>

          <div className="grid">
            <label>
              Standard language
              <select
                value={settings.language}
                onChange={(event) => updateSetting("language", event.target.value as Language)}
              >
                <option value="da">Danish</option>
                <option value="en">English</option>
                <option value="auto">Auto</option>
              </select>
            </label>

            <label>
              Default mode
              <select
                value={settings.defaultMode}
                onChange={(event) => updateSetting("defaultMode", event.target.value as DefaultMode)}
              >
                <option value="voice">Voice to Text</option>
                <option value="linkedin">LinkedIn Reply</option>
                <option value="screen">Screen Assistant</option>
              </select>
            </label>
          </div>

          <label>
            Mikkel Tone
            <textarea value={settings.customTone} onChange={(event) => updateSetting("customTone", event.target.value)} />
          </label>

          <div className="toggles">
            <label>
              <input
                type="checkbox"
                checked={settings.autoInsert}
                onChange={(event) => updateSetting("autoInsert", event.target.checked)}
              />
              Auto-insert result
            </label>
            <label>
              <input
                type="checkbox"
                checked={settings.alwaysShowResult}
                onChange={(event) => updateSetting("alwaysShowResult", event.target.checked)}
              />
              Always show result before inserting
            </label>
            <label>
              <input
                type="checkbox"
                checked={settings.enableScreenReading}
                onChange={(event) => updateSetting("enableScreenReading", event.target.checked)}
              />
              Enable screen reading
            </label>
            <label>
              <input
                type="checkbox"
                checked={settings.confirmBeforeScreenshot}
                onChange={(event) => updateSetting("confirmBeforeScreenshot", event.target.checked)}
              />
              Require confirmation before screenshot
            </label>
            <label>
              <input
                type="checkbox"
                checked={settings.saveHistory}
                onChange={(event) => updateSetting("saveHistory", event.target.checked)}
              />
              Save history locally
            </label>
            <label>
              <input
                type="checkbox"
                checked={settings.mikkelTone}
                onChange={(event) => updateSetting("mikkelTone", event.target.checked)}
              />
              Use Mikkel Tone prompt
            </label>
          </div>

          {history.length > 0 && (
            <div className="history">
              <h3>Seneste svar</h3>
              {history.slice(0, 3).map((item, index) => (
                <button
                  type="button"
                  key={`${item}-${String(index)}`}
                  onClick={() => {
                    setResult(item);
                    setMessage("Valgt fra historik");
                  }}
                >
                  {item}
                </button>
              ))}
            </div>
          )}
        </aside>
      )}
    </main>
  );
}
