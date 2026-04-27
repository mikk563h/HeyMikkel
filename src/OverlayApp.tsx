import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, X, Check, Wand2, Briefcase, RefreshCw } from "lucide-react";
import { useVoiceSession } from "./hooks/useVoiceSession";
import { getSettings } from "./voice/settings";
import type { AppState } from "./voice/types";

function Waveform() {
  return (
    <div className="hm-wave" aria-hidden>
      {Array.from({ length: 24 }).map((_, i) => (
        <span key={i} className="hm-wave-bar" style={{ animationDelay: `${i * 0.04}s` }} />
      ))}
    </div>
  );
}

function pillCopy(state: AppState, message: string) {
  if (state === "reading") return "Hey Mikkel kigger på skærmen…";
  if (message.includes("Transskriber")) return "Hey Mikkel transskriberer…";
  return "Hey Mikkel tænker…";
}

type WebSpeechResultEvent = {
  resultIndex: number;
  results: { length: number; [i: number]: { 0: { transcript: string } } };
};

function useLiveTranscript(active: boolean, setText: (s: string) => void) {
  const recRef = useRef<{ stop: () => void } | null>(null);

  useEffect(() => {
    if (!active) {
      if (recRef.current) {
        try {
          recRef.current.stop();
        } catch {
          /* ignore */
        }
        recRef.current = null;
      }
      return;
    }

    const W = window as unknown as {
      webkitSpeechRecognition?: new () => {
        lang: string;
        interimResults: boolean;
        continuous: boolean;
        start: () => void;
        stop: () => void;
        onresult: ((e: WebSpeechResultEvent) => void) | null;
        onerror: (() => void) | null;
      };
    };
    const Ctor = W.webkitSpeechRecognition;
    if (!Ctor) {
      setText("Lytter…");
      return;
    }

    const recognition = new Ctor();
    recRef.current = recognition;
    recognition.lang = "da-DK";
    recognition.interimResults = true;
    recognition.continuous = true;
    recognition.onresult = (raw) => {
      const event = raw as WebSpeechResultEvent;
      let t = "";
      for (let i = event.resultIndex; i < event.results.length; i += 1) {
        t += event.results[i][0].transcript;
      }
      setText(t.trim() || "Lytter…");
    };
    recognition.onerror = () => {
      setText("Lytter…");
    };
    try {
      recognition.start();
    } catch {
      setText("Lytter…");
    }

    return () => {
      try {
        recognition.stop();
      } catch {
        /* ignore */
      }
    };
  }, [active, setText]);
}

export function OverlayApp() {
  const [liveLine, setLiveLine] = useState("");

  const voice = useVoiceSession({ listenPushToTalk: true });

  const {
    state,
    message,
    result,
    setResult,
    error,
    resetToIdle,
    copyResult,
    insertResult,
    refine,
    generateResponse,
    lastInstruction,
    lastScreenshot,
  } = voice;

  useLiveTranscript(state === "listening", setLiveLine);

  useEffect(() => {
    if (!getSettings().apiKey.trim()) {
      void invoke("show_settings_window");
    }
  }, []);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    if (state !== "idle") return;
    void (async () => {
      try {
        await invoke("set_overlay_interactive", { interactive: false });
        await getCurrentWindow().hide();
      } catch {
        /* ignore */
      }
    })();
  }, [state]);

  const dismissResult = useCallback(async () => {
    setResult("");
    resetToIdle();
    try {
      await invoke("set_overlay_interactive", { interactive: false });
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }, [resetToIdle, setResult]);

  const onDismissError = useCallback(() => {
    resetToIdle();
    void (async () => {
      try {
        await getCurrentWindow().hide();
      } catch {
        /* ignore */
      }
    })();
  }, [resetToIdle]);

  const hasResultPanel = state === "result" && Boolean(result);
  const showThinkPill = state === "thinking" || state === "reading";

  return (
    <div className="hm-overlay">
      {state === "listening" && (
        <>
          <div className="hm-bubble">
            <p className="hm-bubble-text">{liveLine || "Lytter…"}</p>
            <span className="hm-cursor" aria-hidden />
          </div>
          <div className="hm-dock">
            <div className="hm-dock-inner">
              <Waveform />
            </div>
          </div>
        </>
      )}

      {showThinkPill && (
        <div className="hm-dock">
          <div className="hm-dock-inner hm-dock-inner--think">
            <span className="hm-think-label">{pillCopy(state, message)}</span>
          </div>
        </div>
      )}

      {hasResultPanel && result && (
        <div className="hm-result">
          <header className="hm-result-hd">
            <span className="hm-mark">HEY MIKKEL</span>
            <div className="hm-result-tools">
              <button type="button" className="hm-icon" title="Kopiér" onClick={() => copyResult()}>
                <Copy size={18} />
              </button>
              <button type="button" className="hm-icon" title="Luk" onClick={() => void dismissResult()}>
                <X size={18} />
              </button>
            </div>
          </header>
          <p className="hm-result-body">{result}</p>
          <div className="hm-result-chips">
            <button type="button" onClick={() => void insertResult()}>
              <Check size={15} />
              Indsæt
            </button>
            <button type="button" onClick={() => void refine("Gør svaret kortere. Return only the final text.")}>
              <Wand2 size={15} />
              Kortere
            </button>
            <button type="button" onClick={() => void refine("Gør svaret mere personligt. Return only the final text.")}>
              Mere personlig
            </button>
            <button type="button" onClick={() => void refine("Gør svaret mere professionelt. Return only the final text.")}>
              <Briefcase size={15} />
              Mere professionel
            </button>
            <button type="button" onClick={() => void generateResponse(lastInstruction, lastScreenshot)}>
              <RefreshCw size={15} />
              Prøv igen
            </button>
          </div>
        </div>
      )}

      {state === "error" && error && (
        <div className="hm-toast">
          <p>{error}</p>
          <div className="hm-toast-actions">
            {/mikrofon|microphone|not allowed|afvist/i.test(error) && window.__TAURI_INTERNALS__ ? (
              <button
                type="button"
                className="hm-toast-btn hm-toast-btn--secondary"
                onClick={() => void invoke("open_microphone_privacy")}
              >
                Mikrofon-tilladelse
              </button>
            ) : null}
            <button type="button" className="hm-toast-btn" onClick={onDismissError}>
              OK
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
