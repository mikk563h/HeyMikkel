import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { requestNativeMicrophonePermission } from "../macosMicPermission";
import {
  acquireUserAudio,
  audioBlobToBase64,
  base64ToWavBlob,
  describeMicError,
} from "../voice/audioUtils";
import { buildPrompt, shouldReadScreen } from "../voice/prompts";
import type { AppState } from "../voice/types";
import { getSettings } from "../voice/settings";

type Options = {
  /** Kun overlay bør lytte, ellers dobbelt optagelse med main-vindue. */
  listenPushToTalk: boolean;
};

export function useVoiceSession(options: Options) {
  const { listenPushToTalk } = options;

  const [state, setState] = useState<AppState>("idle");
  const [message, setMessage] = useState("Klar");
  const [transcript, setTranscript] = useState("");
  const [result, setResult] = useState("");
  const [error, setError] = useState("");
  const [lastScreenshot, setLastScreenshot] = useState<string | null>(null);
  const [lastInstruction, setLastInstruction] = useState("");
  const [history, setHistory] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem("hey-mikkel-history") ?? "[]");
    } catch {
      return [];
    }
  });

  const mediaRecorder = useRef<MediaRecorder | null>(null);
  const audioChunks = useRef<Blob[]>([]);
  const isRecording = useRef(false);
  /** Tauri: Core Audio (cpal) — ikke WebView getUserMedia (0 enheder på macOS). */
  const useNativeMicRef = useRef(false);

  const insertResult = useCallback(
    async (text?: string) => {
      const payload = (text ?? result).trim();
      if (!payload) return;
      await invoke("insert_text", { text: payload });
      setMessage("Indsat ved cursor");
    },
    [result],
  );

  const generateResponse = useCallback(
    async (instruction: string, screenshotBase64?: string | null) => {
      const settings = getSettings();
      const { systemPrompt, userPrompt } = buildPrompt(instruction, settings, Boolean(screenshotBase64));
      setState("thinking");
      setMessage("Skriver svar...");

      const text = await invoke<string>("ask_ai", {
        apiKey: settings.apiKey,
        systemPrompt,
        userPrompt,
        screenshotBase64: screenshotBase64 ?? null,
      });

      const trimmed = text.trim();
      setResult(trimmed);
      setLastInstruction(instruction);
      setState("result");
      setMessage("Svar klar");

      if (settings.saveHistory) {
        setHistory((items) => {
          const next = [trimmed, ...items].slice(0, 10);
          localStorage.setItem("hey-mikkel-history", JSON.stringify(next));
          return next;
        });
      }

      const shouldAutoInsert = settings.autoInsert && (!screenshotBase64 || !settings.alwaysShowResult);
      if (shouldAutoInsert) {
        await insertResult(trimmed);
      }
    },
    [insertResult],
  );

  const finishRecording = useCallback(
    async (blob: Blob) => {
      const settings = getSettings();
      if (!settings.apiKey.trim()) {
        throw new Error("Indsæt din OpenAI API key i Indstillinger først.");
      }

      setState("thinking");
      setMessage("Transskriberer...");
      const audioBase64 = await audioBlobToBase64(blob);
      const spokenText = await invoke<string>("transcribe_audio", {
        apiKey: settings.apiKey,
        audioBase64,
        mimeType: blob.type || "audio/webm",
      });

      setTranscript(spokenText);

      let screenshotBase64: string | null = null;
      if (shouldReadScreen(spokenText, settings)) {
        if (
          settings.confirmBeforeScreenshot &&
          !window.confirm("Hey Mikkel vil tage et screenshot af den aktive skærm. Fortsæt?")
        ) {
          setState("idle");
          setMessage("Screenshot annulleret");
          return;
        }

        setState("reading");
        setMessage("Kigger på skærmen...");
        screenshotBase64 = await invoke<string>("capture_screen");
        setLastScreenshot(screenshotBase64);
      } else {
        setLastScreenshot(null);
      }

      await generateResponse(spokenText, screenshotBase64);
    },
    [generateResponse],
  );

  const stopRecording = useCallback(() => {
    if (useNativeMicRef.current) {
      if (!isRecording.current) return;
      isRecording.current = false;
      useNativeMicRef.current = false;
      void (async () => {
        try {
          const audioBase64 = await invoke<string>("native_mic_stop");
          const blob = base64ToWavBlob(audioBase64);
          await finishRecording(blob);
        } catch (caught) {
          const text = caught instanceof Error ? caught.message : String(caught);
          setError(text);
          setMessage(text);
          setState("error");
        }
      })();
      return;
    }
    if (!mediaRecorder.current || !isRecording.current) return;
    isRecording.current = false;
    mediaRecorder.current.stop();
  }, [finishRecording]);

  const startRecording = useCallback(async () => {
    if (isRecording.current) return;

    try {
      setError("");
      setResult("");
      setTranscript("");
      setState("listening");
      setMessage("Lytter...");

      if (window.__TAURI_INTERNALS__) {
        await requestNativeMicrophonePermission();
        await invoke("native_mic_start");
        useNativeMicRef.current = true;
        isRecording.current = true;
        return;
      }

      const stream = await acquireUserAudio();
      const recorder = new MediaRecorder(stream);
      audioChunks.current = [];

      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) audioChunks.current.push(event.data);
      };

      recorder.onstop = async () => {
        stream.getTracks().forEach((track) => track.stop());
        const blob = new Blob(audioChunks.current, { type: recorder.mimeType || "audio/webm" });
        try {
          await finishRecording(blob);
        } catch (caught) {
          const text = caught instanceof Error ? caught.message : String(caught);
          setError(text);
          setMessage(text);
          setState("error");
        }
      };

      mediaRecorder.current = recorder;
      isRecording.current = true;
      recorder.start();
    } catch (caught) {
      const text = describeMicError(caught);
      setError(text);
      setMessage("Kunne ikke starte mikrofonen");
      setState("error");
    }
  }, [finishRecording]);

  const testMicrophone = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__) {
      setMessage("Åbn Tauri app-vinduet (ikke browser-preview).");
      return;
    }
    setMessage("Test mikrofon...");
    setError("");
    try {
      await requestNativeMicrophonePermission();
      await invoke("native_mic_start");
      await new Promise((r) => setTimeout(r, 450));
      const audioBase64 = await invoke<string>("native_mic_stop");
      if (audioBase64.length < 200) {
        throw new Error("Fik næsten ingen lyd — tjek at du har sagt ja til mikrofon, og prøv igen.");
      }
      setMessage("Mikrofon virker (Core Audio).");
    } catch (caught) {
      setError(describeMicError(caught));
      setMessage("Mikrofon fejlede.");
    }
  }, []);

  useEffect(() => {
    if (!listenPushToTalk || !window.__TAURI_INTERNALS__) return;

    let active = true;
    let unlistenStart: (() => void) | undefined;
    let unlistenStop: (() => void) | undefined;
    let unlistenPermission: (() => void) | undefined;

    async function bind() {
      try {
        unlistenStart = await listen("push-to-talk-start", () => {
          if (!active) return;
          void startRecording();
        });
        unlistenStop = await listen("push-to-talk-stop", () => {
          if (!active) return;
          stopRecording();
        });
        unlistenPermission = await listen("push-to-talk-permission-missing", () => {
          if (!active) return;
          setMessage("Giv Accessibility-tilladelse for Hey Mikkel og genstart appen.");
        });
      } catch (e) {
        console.warn("PTT listen failed", e);
      }
    }
    void bind();

    return () => {
      active = false;
      unlistenStart?.();
      unlistenStop?.();
      unlistenPermission?.();
    };
  }, [listenPushToTalk, startRecording, stopRecording]);

  const refine = useCallback(
    async (instruction: string) => {
      if (!result.trim()) return;
      await generateResponse(
        `${lastInstruction}\n\nRewrite this result: ${result}\n\nNew instruction: ${instruction}`,
        lastScreenshot,
      );
    },
    [generateResponse, lastInstruction, lastScreenshot, result],
  );

  const copyResult = useCallback(
    async (text = result) => {
      if (!text.trim()) return;
      await invoke("copy_text", { text });
      setMessage("Kopieret");
    },
    [result],
  );

  const resetToIdle = useCallback(() => {
    setState("idle");
    setError("");
    setResult("");
    setTranscript("");
    setMessage("Klar");
  }, []);

  return {
    state,
    setState,
    message,
    setMessage,
    transcript,
    setTranscript,
    result,
    setResult,
    error,
    setError,
    lastScreenshot,
    lastInstruction,
    history,
    setHistory,
    startRecording,
    stopRecording,
    testMicrophone,
    generateResponse,
    refine,
    copyResult,
    insertResult,
    resetToIdle,
  };
}
