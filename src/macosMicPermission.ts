/**
 * macOS: AVFoundation (ekstra TCC-registrering). Fejl her blokerer ikke — Core Audio (cpal) prøves bagefter.
 */
import { invoke } from "@tauri-apps/api/core";

export async function requestNativeMicrophonePermission(): Promise<void> {
  if (!window.__TAURI_INTERNALS__) return;
  try {
    await invoke("request_macos_av_microphone");
  } catch (e) {
    console.warn("[Hey Mikkel] request_macos_av_microphone (valgfri):", e);
  }
}
